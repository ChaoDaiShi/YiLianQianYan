import { useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, MessagesSquare, Monitor, PanelLeft } from "lucide-react";
import { Link, NavLink } from "react-router-dom";
import PageShell, { type PageShellProps } from "../../components/layout/PageShell";
import { Button, Modal } from "../../components/ui";
import { channelPlatforms, type ChannelPlatform } from "../channels/channelsModel";
import "./software.css";

export type PlatformContent = (closeNavigation?: () => void) => ReactNode;

type CategoryProps = {
  platform?: ChannelPlatform;
  platformContent?: PlatformContent;
  onPlatformChange?: (id: ChannelPlatform, closeNavigation?: () => void) => void;
};

function SoftwareCategories({ platform, platformContent, onPlatformChange, onNavigate }: CategoryProps & { onNavigate?: () => void }) {
  return <nav aria-label="软件分类" className="software-category-nav">
    <ul>
      <li><NavLink to="/software" end onClick={onNavigate}><Monitor size={15} aria-hidden="true" />桌面软件</NavLink></li>
      <li>
        <NavLink to="/software/channels" onClick={onNavigate}><MessagesSquare size={15} aria-hidden="true" />消息渠道</NavLink>
        <ul aria-label="消息渠道平台" className="software-category-platforms">
          {channelPlatforms.map(({ id, label }) => {
            const current = platform === id;
            const expanded = current && Boolean(platformContent);
            const Chevron = expanded ? ChevronDown : ChevronRight;
            return <li key={id}>
              {onPlatformChange ? <button type="button" aria-current={current ? "true" : undefined} aria-expanded={expanded}
                onClick={() => onPlatformChange(id, onNavigate)}>{label}<Chevron size={13} aria-hidden="true" /></button>
                : <Link to={`/software/channels?platform=${id}`} onClick={onNavigate}>{label}</Link>}
              {expanded && platformContent?.(onNavigate)}
            </li>;
          })}
        </ul>
      </li>
    </ul>
  </nav>;
}

export default function SoftwareMountShell({ children, actions, description, ...categories }: Pick<PageShellProps, "children" | "actions" | "description"> & CategoryProps) {
  const [categoriesOpen, setCategoriesOpen] = useState(false);
  const navigationTitle = categories.platform ? "分类与会话" : "软件分类";
  return <PageShell title="软件挂载" description={description} variant="workspace" maxWidth="none" className="software-page"
    actions={<>
      <Button variant="ghost" size="sm" className="software-category-toggle" aria-haspopup="dialog" onClick={() => setCategoriesOpen(true)}><PanelLeft size={15} />{categories.platform ? "会话" : "分类"}</Button>
      {actions}
    </>}>
    <div className={"software-mount-layout" + (categories.platform ? " has-channel-navigation" : "")}>
      <aside className="software-category-sidebar"><SoftwareCategories {...categories} /></aside>
      <div className="software-mount-content">{children}</div>
    </div>
    <Modal open={categoriesOpen} title={navigationTitle} onClose={() => setCategoriesOpen(false)}>
      <SoftwareCategories {...categories} onNavigate={() => setCategoriesOpen(false)} />
    </Modal>
  </PageShell>;
}
