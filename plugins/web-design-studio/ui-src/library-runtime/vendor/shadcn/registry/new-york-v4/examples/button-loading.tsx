// @ts-nocheck
import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"
import { Spinner } from "@shadcn-registry/registry/new-york-v4/ui/spinner"

export default function ButtonLoading() {
  return (
    <Button size="sm" variant="outline" disabled>
      <Spinner />
      Submit
    </Button>
  )
}
