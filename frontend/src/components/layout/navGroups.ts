import type { LucideIcon } from "lucide-react";
import { DERIVED_NAV_GROUPS } from "./navRegistry";
export interface NavGroupItem {
  readonly to: string;
  readonly id: string;
  readonly label: string;
  readonly icon: LucideIcon;
}

export interface NavGroup {
  readonly label: string;
  readonly items: readonly NavGroupItem[];
}

export const NAV_GROUPS: readonly NavGroup[] = DERIVED_NAV_GROUPS;
