import { useState } from "react";
import { MessagesSquare, Monitor, PanelLeft } from "lucide-react";
import { Link, NavLink } from "react-router-dom";
import PageShell, { type PageShellProps } from "../../components/layout/PageShell";
import { Button, Modal } from "../../components/ui";
import "./software.css";

function SoftwareCategories({onNavigate}: {onNavigate?: () => void}) {
  return <nav aria-label="软件分类" className="software-category-nav"><ul>
    <li><NavLink to="/software" end onClick={onNavigate}><Monitor size={15} aria-hidden="true" />桌面软件</NavLink></li>
    <li><Link to="/channels" onClick={onNavigate}><MessagesSquare size={15} aria-hidden="true" />消息渠道</Link></li>
  </ul></nav>;
}

export default function SoftwareMountShell({ children, actions, description }: Pick<PageShellProps, "children" | "actions" | "description">) {
  const [categoriesOpen, setCategoriesOpen] = useState(false);
  return <PageShell title="软件挂载" description={description} variant="workspace" maxWidth="none" className="software-page"
    actions={<><Button variant="ghost" size="sm" className="software-category-toggle" aria-haspopup="dialog" onClick={() => setCategoriesOpen(true)}><PanelLeft size={15} />分类</Button>{actions}</>}>
    <div className="software-mount-layout">
      <aside className="software-category-sidebar"><SoftwareCategories /></aside>
      <div className="software-mount-content">{children}</div>
    </div>
    <Modal open={categoriesOpen} title="软件分类" onClose={() => setCategoriesOpen(false)}>
      <SoftwareCategories onNavigate={() => setCategoriesOpen(false)} />
    </Modal>
  </PageShell>;
}
