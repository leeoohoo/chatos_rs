// @ts-nocheck
import { Label } from "@shadcn-registry/registry/new-york-v4/ui/label"
import { Switch } from "@shadcn-registry/registry/new-york-v4/ui/switch"

export default function SwitchDemo() {
  return (
    <div className="flex items-center space-x-2">
      <Switch id="airplane-mode" />
      <Label htmlFor="airplane-mode">Airplane Mode</Label>
    </div>
  )
}
