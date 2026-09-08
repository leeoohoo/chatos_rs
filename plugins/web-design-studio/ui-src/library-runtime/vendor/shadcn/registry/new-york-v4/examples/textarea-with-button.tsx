// @ts-nocheck
import { Button } from "@shadcn-registry/registry/new-york-v4/ui/button"
import { Textarea } from "@shadcn-registry/registry/new-york-v4/ui/textarea"

export default function TextareaWithButton() {
  return (
    <div className="grid w-full gap-2">
      <Textarea placeholder="Type your message here." />
      <Button>Send message</Button>
    </div>
  )
}
