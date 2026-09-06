// @ts-nocheck
import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"
import {
  ButtonGroup,
  ButtonGroupSeparator,
} from "@shadcn-registry/registry/new-york-v4/ui/button-group"

export default function ButtonGroupSeparatorDemo() {
  return (
    <ButtonGroup>
      <Button variant="secondary" size="sm">
        Copy
      </Button>
      <ButtonGroupSeparator />
      <Button variant="secondary" size="sm">
        Paste
      </Button>
    </ButtonGroup>
  )
}
