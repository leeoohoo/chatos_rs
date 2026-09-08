// @ts-nocheck
import { Italic } from "lucide-react"

import { Toggle } from "@shadcn-registry/registry/new-york-v4/ui/toggle"

export default function ToggleOutline() {
  return (
    <Toggle variant="outline" aria-label="Toggle italic">
      <Italic />
    </Toggle>
  )
}
