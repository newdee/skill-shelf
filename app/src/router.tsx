import {
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
} from "@tanstack/react-router";
import { Boxes, Languages } from "lucide-react";
import { AccountMenu } from "./components/AccountMenu";
import { Button } from "@/components/ui/button";
import { useAuth } from "./lib/auth";
import { useT } from "./lib/i18n";
import { SkillList } from "./pages/SkillList";
import { SkillDetail } from "./pages/SkillDetail";
import { RouteTester } from "./pages/RouteTester";
import { Settings } from "./pages/Settings";
import { ConfigCenter } from "./pages/ConfigCenter";

const navLink =
  "rounded-full px-3 py-1.5 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground";
const navLinkActive = "bg-accent text-foreground";

function Layout() {
  // Config center is admin-only; only surface the link when the user can write.
  const { canWrite } = useAuth();
  const { t, lang, setLang } = useT();
  return (
    <div className="min-h-svh">
      <nav className="glass sticky top-0 z-20 border-b border-border/60 bg-background/70 backdrop-blur-xl backdrop-saturate-150">
        <div className="mx-auto flex h-14 max-w-3xl items-center gap-1 px-4">
          <Link to="/" className="mr-3 flex items-center gap-2 font-heading font-semibold tracking-tight">
            <Boxes className="size-5 text-primary" /> Skill Shelf
          </Link>
          <Link to="/" className={navLink} activeProps={{ className: navLinkActive }} activeOptions={{ exact: true }}>
            {t("nav.skills")}
          </Link>
          <Link to="/route" className={navLink} activeProps={{ className: navLinkActive }}>
            {t("nav.route")}
          </Link>
          {canWrite && (
            <Link to="/config" className={navLink} activeProps={{ className: navLinkActive }}>
              {t("nav.config")}
            </Link>
          )}
          <div className="flex-1" />
          <Button
            variant="ghost"
            size="sm"
            className="gap-1.5 font-mono text-xs"
            aria-label={lang === "zh" ? "Switch to English" : "切换到中文"}
            onClick={() => setLang(lang === "zh" ? "en" : "zh")}
          >
            <Languages className="size-3.5" /> {lang === "zh" ? "EN" : "中"}
          </Button>
          <AccountMenu />
        </div>
      </nav>
      <main className="mx-auto max-w-3xl px-4 py-8">
        <Outlet />
      </main>
    </div>
  );
}

const rootRoute = createRootRoute({ component: Layout });

const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: SkillList });
const skillRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skill/$id",
  component: SkillDetail,
});
const routeRoute = createRoute({ getParentRoute: () => rootRoute, path: "/route", component: RouteTester });
const configRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/config",
  component: ConfigCenter,
});
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: Settings,
});

const routeTree = rootRoute.addChildren([indexRoute, skillRoute, routeRoute, configRoute, settingsRoute]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
