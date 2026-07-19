import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { CircleUser, LogIn, LogOut, Monitor, Moon, Settings as SettingsIcon, Sun } from "lucide-react";
import { toast } from "sonner";
import { useAuth } from "../lib/auth";
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

/** The single top-right menu: Settings, Theme, and (on auth instances) sign
 * in / out. Trigger shows the username when signed in. */
export function AccountMenu() {
  const { authEnabled, user, signIn, signUp, signOut } = useAuth();
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
      toast.success(kind === "in" ? "Signed in" : "Account created");
    } catch (e) {
      toast.error(`${kind === "in" ? "Sign in" : "Sign up"} failed: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="gap-1.5 rounded-full" title="Menu">
            <CircleUser className="size-4" />
            {user && <span className="max-sm:hidden">{user.username}</span>}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-48">
          {authEnabled && user && (
            <>
              <DropdownMenuLabel className="flex items-center gap-2">
                {user.username}
                <Badge variant={user.role === "admin" ? "secondary" : "outline"}>{user.role}</Badge>
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
            </>
          )}

          <DropdownMenuItem asChild>
            <Link to="/settings">
              <SettingsIcon />
              Settings
            </Link>
          </DropdownMenuItem>

          <DropdownMenuItem onSelect={(e) => { e.preventDefault(); cycleTheme(); }}>
            <ThemeIcon />
            Theme
            <span className="ml-auto text-xs capitalize text-muted-foreground">{theme}</span>
          </DropdownMenuItem>

          {authEnabled && (
            <>
              <DropdownMenuSeparator />
              {user ? (
                <DropdownMenuItem onClick={signOut}>
                  <LogOut />
                  Sign out
                </DropdownMenuItem>
              ) : (
                <DropdownMenuItem onSelect={() => setDialog(true)}>
                  <LogIn />
                  Sign in
                </DropdownMenuItem>
              )}
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialog} onOpenChange={setDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Sign in</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="au">Username</FieldLabel>
              <Input id="au" value={username} onChange={(e) => setUsername(e.target.value)} />
            </Field>
            <Field>
              <FieldLabel htmlFor="ap">Password</FieldLabel>
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
              Sign up
            </Button>
            <Button disabled={busy || invalid} onClick={() => doAuth("in")}>
              Sign in
            </Button>
          </DialogFooter>
          <p className="text-xs text-muted-foreground">First account becomes admin. Password ≥ 6 characters.</p>
        </DialogContent>
      </Dialog>
    </>
  );
}
