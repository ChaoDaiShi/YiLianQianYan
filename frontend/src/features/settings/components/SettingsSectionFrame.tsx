import type { ReactNode } from "react";
import type { SettingsGroup } from "../model/settingsNavigation";

interface SettingsSectionFrameProps {
  group: SettingsGroup;
  children: ReactNode;
}

export default function SettingsSectionFrame({ group, children }: SettingsSectionFrameProps) {
  const Icon = group.icon;
  return (
    <section className="settings-section-frame" aria-labelledby={`settings-${group.key}-title`}>
      <header className="settings-section-intro">
        <span className="settings-section-icon" data-accent={group.accent} aria-hidden="true">
          <Icon />
        </span>
        <div className="min-w-0">
          <p className="settings-section-eyebrow">设置分类</p>
          <h2 id={`settings-${group.key}-title`}>{group.title}</h2>
          <p>{group.description}</p>
        </div>
      </header>
      <div className="settings-card-grid">{children}</div>
    </section>
  );
}
