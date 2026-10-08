import type { ReactNode } from "react";
import { Icon } from "./Icon";
import mark from "../assets/pitscript-mark.png";

export type Page = "home" | "new" | "meeting" | "settings";

type AppShellProps = {
  page: Page;
  onNavigate: (page: Page) => void;
  children: ReactNode;
  recording?: boolean;
};

const navItems = [
  { page: "home", label: "Biblioteca", icon: "folder" },
  { page: "new", label: "Nova gravação", icon: "plus" },
  { page: "settings", label: "Configurações", icon: "settings" },
] as const;

export function AppShell({
  page,
  onNavigate,
  children,
  recording,
}: AppShellProps) {
  return (
    <div className={`app-layout${recording ? " app-layout--recording" : ""}`}>
      <aside className="sidebar">
        <div className="brand" aria-label="PitScript">
          <img className="brand-mark" src={mark} alt="" />
          <span>PitScript</span>
        </div>

        <nav className="nav" aria-label="Navegação principal">
          {navItems.map((item) => (
            <button
              key={item.page}
              type="button"
              className={`nav-link ${page === item.page ? "nav-link--active" : ""}`}
              aria-current={
                page === item.page ||
                (page === "meeting" && item.page === "home")
                  ? "page"
                  : undefined
              }
              onClick={() => onNavigate(item.page)}
            >
              <Icon name={item.icon} size={20} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-note">Tudo fica neste computador.</div>
      </aside>

      <main className="main-content" id="conteudo">
        {children}
      </main>
    </div>
  );
}
