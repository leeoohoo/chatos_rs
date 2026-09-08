// @ts-nocheck
import { Checkbox } from "@shadcn-registry/registry/new-york-v4/ui/checkbox"
import { Label } from "@shadcn-registry/registry/new-york-v4/ui/label"

export default function LabelDemo() {
  return (
    <div>
      <div className="flex items-center space-x-2">
        <Checkbox id="terms" />
        <Label htmlFor="terms">Accept terms and conditions</Label>
      </div>
    </div>
  )
}
