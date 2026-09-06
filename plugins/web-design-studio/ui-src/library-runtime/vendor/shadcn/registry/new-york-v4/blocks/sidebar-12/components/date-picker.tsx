// @ts-nocheck
import { Calendar } from "@shadcn-registry/registry/new-york-v4/ui/calendar"
import {
  SidebarGroup,
  SidebarGroupContent,
} from "@shadcn-registry/registry/new-york-v4/ui/sidebar"

export function DatePicker() {
  return (
    <SidebarGroup className="px-0">
      <SidebarGroupContent>
        <Calendar className="[&_[role=gridcell]]:w-[33px] [&_[role=gridcell].bg-accent]:bg-sidebar-primary [&_[role=gridcell].bg-accent]:text-sidebar-primary-foreground" />
      </SidebarGroupContent>
    </SidebarGroup>
  )
}
