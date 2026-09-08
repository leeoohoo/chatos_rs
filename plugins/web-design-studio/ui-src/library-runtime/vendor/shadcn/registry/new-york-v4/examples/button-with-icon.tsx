// @ts-nocheck
import { IconGitBranch } from "@tabler/icons-react"

import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"

export default function ButtonWithIcon() {
  return (
    <Button variant="outline" size="sm">
      <IconGitBranch /> New Branch
    </Button>
  )
}
