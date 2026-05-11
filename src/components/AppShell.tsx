import { Link, useLocation } from "react-router-dom";

interface Props { children: React.ReactNode }

export default function AppShell({ children }: Props) {
  const location = useLocation();
  const isHome = location.pathname === "/";
  return (
    <div className="flex h-screen flex-col">
      <header className="flex h-10 items-center border-b border-border/60 px-4 text-sm">
        <Link to="/" className="font-semibold text-text hover:text-accent">
          EconProject
        </Link>
        {!isHome && (
          <Link to="/" className="ml-3 text-muted hover:text-text">
            ← Saved companies
          </Link>
        )}
        <div className="ml-auto text-xs text-muted">v0.1.0 — local-first</div>
      </header>
      <main className="flex-1 overflow-auto">{children}</main>
    </div>
  );
}
