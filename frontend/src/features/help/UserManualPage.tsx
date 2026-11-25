import ReactMarkdown from "react-markdown";
import { Link } from "react-router-dom";
import PageShell from "../../components/layout/PageShell";
import manual from "./user-manual.md?raw";
import "./userManual.css";

const sections = Array.from(manual.matchAll(/^## (.+)$/gm), match => match[1].trim());

export default function UserManualPage() {
  return <PageShell title="用户手册" maxWidth="reading" className="user-manual-page">
    <nav className="user-manual-toc" aria-label="手册目录">
      {sections.map(title => <a key={title} href={`#${encodeURIComponent(title)}`}>{title}</a>)}
    </nav>
    <article className="user-manual-body">
      <ReactMarkdown components={{
        h2: ({ children }) => <h2 id={String(children)}>{children}</h2>,
        a: ({ href, children }) => href?.startsWith("/") ? <Link to={href}>{children}</Link> : <a href={href}>{children}</a>,
      }}>{manual}</ReactMarkdown>
    </article>
  </PageShell>;
}
