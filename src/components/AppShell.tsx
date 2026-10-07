import type { ReactNode } from "react";
import { Icon } from "./Icon";

export type Page = "home" | "new" | "meeting" | "settings";

type AppShellProps = {
  page: Page;
  onNavigate: (page: Page) => void;
  children: ReactNode;
};

const navItems = [
  { page: "home", label: "Início", icon: "home" },
  { page: "new", label: "Nova gravação", icon: "plus" },
  { page: "settings", label: "Configurações", icon: "settings" },
] as const;

export function AppShell({ page, onNavigate, children }: AppShellProps) {
  return (
    <div className="app-layout">
      <aside className="sidebar">
        <div className="brand" aria-label="Meeting Recorder">
          <span className="brand-mark" aria-hidden="true">
            <span />
            <span />
            <span />
            <span />
          </span>
          <span>Meeting Recorder</span>
        </div>

        <nav className="nav" aria-label="Navegação principal">
          {navItems.map((item) => (
            <button
              key={item.page}
              type="button"
              className={`nav-link ${page === item.page ? "nav-link--active" : ""}`}
              aria-current={page === item.page ? "page" : undefined}
              onClick={() => onNavigate(item.page)}
            >
              <Icon name={item.icon} size={20} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-note">
          <span className="sidebar-note__dot" />
          Gravação local
        </div>
      </aside>

      <main className="main-content" id="conteudo">
        {children}
      </main>
    </div>
  );
}
