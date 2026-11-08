// The origins of the proxied requests, by country, for the admin's world
// map. Each accepted request names its client IP once; the first lookup
// through a MaxMind DB file turns it into a country and a point on the
// map, and every country keeps a count and the average of the points it
// has seen.
//
// The database is not shipped: the environment variable
// `PINGAP_GEOIP_DB` names a MaxMind DB file (GeoLite2 City or Country,
// freely available from MaxMind). Without it the stats stay empty and
// the map page says so. `PINGAP_GEO_DEMO=1` seeds a few countries with
// sample counts, so a fresh install has something to look at — and so
// it is plain on the page that the numbers are not real traffic.

use ahash::AHashMap;
use maxminddb::geoip2;
use serde::Serialize;
use std::net::IpAddr;
use std::sync::{LazyLock, Mutex};

/// The environment variable naming the MaxMind DB file (`.mmdb`).
pub const GEOIP_DB_ENV: &str = "PINGAP_GEOIP_DB";
/// The environment variable that seeds the stats with sample data.
pub const GEO_DEMO_ENV: &str = "PINGAP_GEO_DEMO";

/// One country's share of the recorded requests.
#[derive(Debug, Serialize)]
pub struct GeoCountryStat {
    /// ISO 3166-1 alpha-2 code, e.g. `CN`. Empty when the database
    /// could not name the country.
    pub country: String,
    /// The requests seen from the country.
    pub count: u64,
    /// The average latitude of those requests.
    pub lat: f64,
    /// The average longitude of those requests.
    pub lon: f64,
}

/// What `GET /api/geo_stats` answers.
#[derive(Debug, Serialize)]
pub struct GeoStatsSnapshot {
    /// Whether a database file was named and opened. The map page shows
    /// a hint when this is false.
    pub db_loaded: bool,
    /// The sum of every country's count.
    pub total: u64,
    /// The countries, busiest first.
    pub countries: Vec<GeoCountryStat>,
}

/// The counts and the running point sums of one country.
#[derive(Debug, Default)]
struct CountryEntry {
    count: u64,
    lat_sum: f64,
    lon_sum: f64,
}

/// The database: absent, broken, or open. A broken file is not retried —
/// a wrong path stays wrong, and one log line says so.
enum GeoDb {
    NotConfigured,
    Failed,
    Ready(maxminddb::Reader<Vec<u8>>),
}

static GEO_DB: LazyLock<GeoDb> = LazyLock::new(|| {
    let Some(path) = std::env::var(GEOIP_DB_ENV)
        .ok()
        .filter(|path| !path.is_empty())
    else {
        return GeoDb::NotConfigured;
    };
    match maxminddb::Reader::open_readfile(path.clone()) {
        Ok(reader) => {
            tracing::info!(path, "GeoIP database loaded");
            GeoDb::Ready(reader)
        },
        Err(e) => {
            tracing::warn!(path, error = e.to_string(), "GeoIP database load fail");
            GeoDb::Failed
        },
    }
});

static COUNTRY_STATS: LazyLock<Mutex<AHashMap<String, CountryEntry>>> =
    LazyLock::new(|| {
        let mut map = AHashMap::new();
        if std::env::var(GEO_DEMO_ENV)
            .ok()
            .is_some_and(|value| !value.is_empty() && value != "0")
        {
            for &(country, lat, lon, count) in DEMO_COUNTRIES {
                map.insert(
                    country.to_string(),
                    CountryEntry {
                        count,
                        lat_sum: lat * count as f64,
                        lon_sum: lon * count as f64,
                    },
                );
            }
            tracing::info!(
                "GeoIP demo data seeded ({GEO_DEMO_ENV}=1) - not real traffic"
            );
        }
        Mutex::new(map)
    });

/// (ISO 3166-1 alpha-2, latitude, longitude, sample request count) of the
/// demo seed. The points are the usual centres of the countries.
const DEMO_COUNTRIES: &[(&str, f64, f64, u64)] = &[
    ("CN", 35.0, 103.0, 1240),
    ("US", 39.8, -98.6, 861),
    ("DE", 51.2, 10.4, 325),
    ("JP", 36.2, 138.3, 298),
    ("SG", 1.35, 103.8, 213),
    ("FR", 46.6, 2.4, 182),
    ("GB", 54.0, -2.0, 165),
    ("KR", 36.5, 127.9, 121),
    ("BR", -10.8, -52.9, 96),
    ("AU", -25.3, 133.8, 88),
    ("IN", 22.0, 79.0, 76),
    ("CA", 56.1, -106.3, 54),
    ("RU", 55.7, 37.6, 49),
    ("NL", 52.2, 5.6, 41),
    ("HK", 22.3, 114.2, 37),
    ("AE", 24.0, 54.0, 26),
    ("ZA", -29.0, 24.0, 18),
    ("MX", 23.6, -102.5, 14),
];

/// Whether the address is one a GeoIP database has no country for: the
/// loopback, the private ranges and the like. Counting those would put
/// every local probe on the map.
fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local()
        },
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Records one request's client IP: looks the country up and adds a
/// count. A local address, an unnamed country or an absent database
/// leaves the stats untouched, and none of that is worth a log line —
/// this runs for every accepted request.
pub fn record_client_ip(ip: &str) {
    let Ok(addr) = ip.parse::<IpAddr>() else {
        return;
    };
    if is_local_ip(addr) {
        return;
    }
    let GeoDb::Ready(reader) = &*GEO_DB else {
        return;
    };
    let Ok(result) = reader.lookup(addr) else {
        return;
    };
    let Ok(Some(city)) = result.decode::<geoip2::City>() else {
        return;
    };
    let Some(country) = city.country.iso_code else {
        return;
    };
    let (lat, lon) =
        match (city.location.latitude, city.location.longitude) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => (0.0, 0.0),
        };

    let mut stats = COUNTRY_STATS.lock().unwrap_or_else(|e| e.into_inner());
    let entry = stats.entry(country.to_string()).or_default();
    entry.count += 1;
    entry.lat_sum += lat;
    entry.lon_sum += lon;
}

/// The stats as the admin API serves them: every country, busiest first.
pub fn geo_stats_snapshot() -> GeoStatsSnapshot {
    let db_loaded = matches!(&*GEO_DB, GeoDb::Ready(_));
    let stats = COUNTRY_STATS.lock().unwrap_or_else(|e| e.into_inner());
    let mut countries: Vec<GeoCountryStat> = stats
        .iter()
        .map(|(country, entry)| GeoCountryStat {
            country: country.clone(),
            count: entry.count,
            lat: if entry.count == 0 {
                0.0
            } else {
                entry.lat_sum / entry.count as f64
            },
            lon: if entry.count == 0 {
                0.0
            } else {
                entry.lon_sum / entry.count as f64
            },
        })
        .collect();
    countries.sort_by(|a, b| b.count.cmp(&a.count).then(a.country.cmp(&b.country)));
    let total = countries.iter().map(|c| c.count).sum();
    GeoStatsSnapshot {
        db_loaded,
        total,
        countries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_ip_skipped() {
        // No database in tests: recording is a no-op either way, but the
        // local addresses must not even reach the lookup.
        assert!(is_local_ip("127.0.0.1".parse().unwrap()));
        assert!(is_local_ip("192.168.1.1".parse().unwrap()));
        assert!(is_local_ip("10.0.0.1".parse().unwrap()));
        assert!(!is_local_ip("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn test_demo_seed() {
        // SAFETY: single-threaded test, no other thread reads the env.
        unsafe {
            std::env::set_var(GEO_DEMO_ENV, "1");
        }
        let snapshot = geo_stats_snapshot();
        assert!(snapshot.total > 0);
        assert!(!snapshot.countries.is_empty());
        // Busiest first: the head of the list holds the largest count.
        let first = &snapshot.countries[0];
        assert!(snapshot
            .countries
            .iter()
            .all(|c| c.count <= first.count));
        // SAFETY: as above, and the variable is ours to remove.
        unsafe {
            std::env::remove_var(GEO_DEMO_ENV);
        }
    }

    #[test]
    fn test_record_without_db_is_noop() {
        let before = geo_stats_snapshot().total;
        record_client_ip("8.8.8.8");
        assert_eq!(geo_stats_snapshot().total, before);
    }
}
