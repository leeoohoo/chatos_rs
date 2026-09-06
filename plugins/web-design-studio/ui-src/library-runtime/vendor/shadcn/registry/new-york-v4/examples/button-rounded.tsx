// @ts-nocheck
import { ArrowUpIcon } from "lucide-react"

import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"

export default function ButtonRounded() {
  return (
    <div className="flex flex-col gap-8">
      <Button variant="outline" size="icon" className="rounded-full">
        <ArrowUpIcon />
      </Button>
    </div>
  )
}
