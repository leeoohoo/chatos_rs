// @ts-nocheck
import { Italic } from "lucide-react"

import { Toggle } from "@shadcn-registry/registry/new-york-v4/ui/toggle"

export default function ToggleWithText() {
  return (
    <Toggle aria-label="Toggle italic">
      <Italic />
      Italic
    </Toggle>
  )
}
