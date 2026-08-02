import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { CircleUser, LogIn, LogOut, Monitor, Moon, Settings as SettingsIcon, Sun } from "lucide-react";
import { toast } from "sonner";
import { useAuth } from "../lib/auth";
import { useT } from "../lib/i18n";
import { getTheme, setTheme, type Theme } from "../lib/theme";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

const THEME_ORDER: Theme[] = ["system", "light", "dark"];
const THEME_ICON = { system: Monitor, light: Sun, dark: Moon };
const THEME_LABEL_KEY: Record<Theme, string> = {
  system: "account.themeSystem",
  light: "account.themeLight",
  dark: "account.themeDark",
};

/** The single top-right menu: Settings, Theme, and (on auth instances) sign
 * in / out. Trigger shows the username when signed in. */
export function AccountMenu() {
  const { authEnabled, user, signIn, signUp, signOut } = useAuth();
  const { t } = useT();
  const [theme, setThemeState] = useState<Theme>(getTheme());
  const [dialog, setDialog] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const invalid = !username.trim() || password.length < 6;

  function cycleTheme() {
    const next = THEME_ORDER[(THEME_ORDER.indexOf(theme) + 1) % THEME_ORDER.length];
    setTheme(next);
    setThemeState(next);
  }
  const ThemeIcon = THEME_ICON[theme];

  async function doAuth(kind: "in" | "up") {
    setBusy(true);
    try {
      await (kind === "in" ? signIn(username, password) : signUp(username, password));
      setDialog(false);
      setPassword("");
      toast.success(kind === "in" ? t("account.signedIn") : t("account.created"));
    } catch (e) {
      toast.error(
        t(kind === "in" ? "account.signInFailed" : "account.signUpFailed", {
          msg: (e as Error).message,
        }),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="gap-1.5 rounded-full" title={t("account.menu")}>
            <CircleUser className="size-4" />
            {user && <span className="max-sm:hidden">{user.username}</span>}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-48">
          {authEnabled && user && (
            <>
              <DropdownMenuLabel className="flex items-center gap-2">
                {user.username}
                <Badge variant={user.role === "admin" ? "secondary" : "outline"}>
                  {user.role === "admin"
                    ? t("account.roleAdmin")
                    : user.role === "user"
                      ? t("account.roleUser")
                      : user.role}
                </Badge>
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
            </>
          )}

          <DropdownMenuItem asChild>
            <Link to="/settings">
              <SettingsIcon />
              {t("account.settings")}
            </Link>
          </DropdownMenuItem>

          <DropdownMenuItem onSelect={(e) => { e.preventDefault(); cycleTheme(); }}>
            <ThemeIcon />
            {t("account.theme")}
            <span className="ml-auto text-xs text-muted-foreground">{t(THEME_LABEL_KEY[theme])}</span>
          </DropdownMenuItem>

          {authEnabled && (
            <>
              <DropdownMenuSeparator />
              {user ? (
                <DropdownMenuItem onClick={signOut}>
                  <LogOut />
                  {t("account.signOut")}
                </DropdownMenuItem>
              ) : (
                <DropdownMenuItem onSelect={() => setDialog(true)}>
                  <LogIn />
                  {t("account.signIn")}
                </DropdownMenuItem>
              )}
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialog} onOpenChange={setDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("account.signIn")}</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="au">{t("account.username")}</FieldLabel>
              <Input id="au" value={username} onChange={(e) => setUsername(e.target.value)} />
            </Field>
            <Field>
              <FieldLabel htmlFor="ap">{t("account.password")}</FieldLabel>
              <Input
                id="ap"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !invalid) doAuth("in");
                }}
              />
            </Field>
          </FieldGroup>
          <DialogFooter>
            <Button variant="outline" disabled={busy || invalid} onClick={() => doAuth("up")}>
              {t("account.signUp")}
            </Button>
            <Button disabled={busy || invalid} onClick={() => doAuth("in")}>
              {t("account.signIn")}
            </Button>
          </DialogFooter>
          <p className="text-xs text-muted-foreground">{t("account.hint")}</p>
        </DialogContent>
      </Dialog>
    </>
  );
}
