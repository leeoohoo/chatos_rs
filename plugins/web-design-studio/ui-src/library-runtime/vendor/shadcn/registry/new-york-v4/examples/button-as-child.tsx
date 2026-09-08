// @ts-nocheck
import Link from "next/link"

import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"

export default function ButtonAsChild() {
  return (
    <Button asChild>
      <Link href="/login">Login</Link>
    </Button>
  )
}
