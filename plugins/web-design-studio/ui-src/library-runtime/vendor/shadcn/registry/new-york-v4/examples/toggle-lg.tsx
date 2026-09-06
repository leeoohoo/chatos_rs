// @ts-nocheck
import { Italic } from "lucide-react"

import { Toggle } from "@shadcn-registry/registry/new-york-v4/ui/toggle"

export default function ToggleLg() {
  return (
    <Toggle size="lg" aria-label="Toggle italic">
      <Italic />
    </Toggle>
  )
}
