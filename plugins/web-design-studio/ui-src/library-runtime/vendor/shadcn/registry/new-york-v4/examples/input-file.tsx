// @ts-nocheck
import { Input } from "@shadcn-registry/registry/new-york-v4/ui/input"
import { Label } from "@shadcn-registry/registry/new-york-v4/ui/label"

export default function InputFile() {
  return (
    <div className="grid w-full max-w-sm items-center gap-3">
      <Label htmlFor="picture">Picture</Label>
      <Input id="picture" type="file" />
    </div>
  )
}
