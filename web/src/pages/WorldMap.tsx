import React from "react";
import * as echarts from "echarts";
import worldJson from "@/assets/world.json";
import request from "@/helpers/request";
import { useI18n } from "@/i18n";
import { PageShell } from "@/components/page-shell";
import { goToLogin } from "@/routers";
import { cn } from "@/lib/utils";
import { Database, Globe2, TriangleAlert } from "lucide-react";

// Registered once — the map data is static, the series data is not.
echarts.registerMap(
  "world",
  worldJson as unknown as Parameters<typeof echarts.registerMap>[1],
);

interface GeoCountryStat {
  country: string;
  count: number;
  lat: number;
  lon: number;
}

interface GeoStats {
  db_loaded: boolean;
  total: number;
  countries: GeoCountryStat[];
}

const POLL_INTERVAL = 10_000;

/** The bubble size is a square root: a country with ten times the
 * requests does not get ten times the circle, which would drown the
 * rest of the map. */
function symbolSize(max: number) {
  return (value: number[]) => {
    const count = value[2] ?? 0;
    if (max <= 0 || count <= 0) {
      return 4;
    }
    return 6 + Math.sqrt(count / max) * 26;
  };
}

export default function WorldMap() {
  const worldMapI18n = useI18n("worldMap");
  const [stats, setStats] = React.useState<GeoStats | null>(null);
  const [loaded, setLoaded] = React.useState(false);
  // The ThemeProvider keeps the intent ("system"), not what it resolved
  // to; the class on the root element is the outcome, watched for here.
  const [dark, setDark] = React.useState(() =>
    document.documentElement.classList.contains("dark"),
  );
  React.useEffect(() => {
    const root = document.documentElement;
    const observer = new MutationObserver(() =>
      setDark(root.classList.contains("dark")),
    );
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  const chartRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    let stopped = false;
    const refresh = () => {
      if (stopped || document.hidden) {
        return;
      }
      request
        .get<GeoStats>("/geo_stats")
        .then(({ data }) => {
          if (!stopped) {
            setStats(data);
            setLoaded(true);
          }
        })
        .catch((err: { status?: number }) => {
          if (err?.status === 401) {
            stopped = true;
            goToLogin();
          }
          // An older process without the endpoint keeps the page quiet,
          // the same as the traffic panel on the dashboard.
          if (!stopped) {
            setLoaded(true);
          }
        });
    };
    refresh();
    const timer = setInterval(refresh, POLL_INTERVAL);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      stopped = true;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, []);

  React.useEffect(() => {
    const el = chartRef.current;
    if (!el) {
      return;
    }
    const chart = echarts.init(el);
    const countries = stats?.countries ?? [];
    const max = countries.reduce((acc, c) => Math.max(acc, c.count), 0);
    chart.setOption({
      backgroundColor: "transparent",
      tooltip: {
        trigger: "item",
        backgroundColor: dark ? "#0f172a" : "#ffffff",
        borderColor: dark ? "#334155" : "#e2e8f0",
        textStyle: {
          color: dark ? "#e2e8f0" : "#334155",
          fontSize: 12,
        },
        formatter: (params: {
          data?: { name: string; value: number[] };
        }) => {
          const data = params.data;
          if (!data) {
            return "";
          }
          const count = data.value?.[2] ?? 0;
          return `<b>${data.name}</b><br/>${worldMapI18n("requests")}: ${count.toLocaleString()}`;
        },
      },
      geo: {
        map: "world",
        roam: true,
        zoom: 1.2,
        scaleLimit: { min: 1, max: 8 },
        itemStyle: {
          areaColor: dark ? "#1e293b" : "#f1f5f9",
          borderColor: dark ? "#475569" : "#cbd5e1",
          borderWidth: 0.5,
        },
        emphasis: {
          label: { show: false },
          itemStyle: {
            areaColor: dark ? "#334155" : "#e2e8f0",
          },
        },
        select: { disabled: true },
      },
      series: [
        {
          name: worldMapI18n("title"),
          type: "effectScatter",
          coordinateSystem: "geo",
          data: countries.map((c) => ({
            name: c.country,
            value: [c.lon, c.lat, c.count],
          })),
          symbolSize: symbolSize(max),
          rippleEffect: { brushType: "stroke", scale: 2.5 },
          itemStyle: { color: "#f97316", opacity: 0.85 },
          emphasis: { scale: 1.4 },
        },
      ],
    });
    const resize = () => chart.resize();
    window.addEventListener("resize", resize);
    return () => {
      window.removeEventListener("resize", resize);
      chart.dispose();
    };
  }, [dark, stats, worldMapI18n]);

  const countries = stats?.countries ?? [];
  const top = countries.slice(0, 10);

  const summary = [
    {
      icon: Globe2,
      label: worldMapI18n("totalRequests"),
      value: (stats?.total ?? 0).toLocaleString(),
    },
    {
      icon: Database,
      label: worldMapI18n("countriesCount"),
      value: countries.length.toString(),
    },
  ];

  return (
    <PageShell
      title={worldMapI18n("title")}
      description={worldMapI18n("description")}
    >
      <div className="flex flex-wrap gap-3">
        {summary.map((item) => (
          <div
            key={item.label}
            className="flex min-w-40 flex-1 items-center gap-3 rounded-lg border border-border bg-card px-4 py-3"
          >
            <item.icon className="size-5 text-muted-foreground" />
            <div>
              <p className="text-xl font-semibold tabular-nums">
                {item.value}
              </p>
              <p className="text-xs text-muted-foreground">{item.label}</p>
            </div>
          </div>
        ))}
      </div>

      {stats && !stats.db_loaded && (
        <div className="mt-4 flex items-start gap-2 rounded-lg border border-border bg-card px-4 py-3 text-[13px] text-muted-foreground">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warn" />
          <span>{worldMapI18n("dbNotConfigured")}</span>
        </div>
      )}

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-[1fr_300px]">
        <div className="overflow-hidden rounded-lg border border-border bg-card">
          <div ref={chartRef} className="h-[520px] w-full" />
        </div>
        <div className="overflow-hidden rounded-lg border border-border bg-card">
          <div className="border-b border-border px-4 py-2.5">
            <h2 className="region-label">{worldMapI18n("topCountries")}</h2>
          </div>
          {!loaded ? (
            <p className="px-4 py-3.5 text-[13px] text-muted-foreground">
              …
            </p>
          ) : top.length === 0 ? (
            <p className="px-4 py-3.5 text-[13px] text-muted-foreground">
              {worldMapI18n("noData")}
            </p>
          ) : (
            <ul className="divide-y divide-border">
              {top.map((c, index) => {
                const share =
                  stats && stats.total > 0
                    ? ((c.count / stats.total) * 100).toFixed(1)
                    : "0";
                return (
                  <li
                    key={c.country}
                    className="flex items-center gap-3 px-4 py-2.5 text-[13px]"
                  >
                    <span className="w-5 text-right font-medium tabular-nums text-muted-foreground">
                      {index + 1}
                    </span>
                    <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
                      {c.country || "??"}
                    </span>
                    <span className="ml-auto font-medium tabular-nums">
                      {c.count.toLocaleString()}
                    </span>
                    <span
                      className={cn(
                        "w-14 text-right text-xs tabular-nums text-muted-foreground",
                      )}
                    >
                      {share}%
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      <p className="mt-3 text-xs text-muted-foreground">
        {worldMapI18n("pollHint")}
      </p>
    </PageShell>
  );
}
