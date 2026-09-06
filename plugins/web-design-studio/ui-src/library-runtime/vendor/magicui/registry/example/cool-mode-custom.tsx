// @ts-nocheck
import { Button } from "@magic/components/ui/button"
import { CoolMode } from "@magic/registry/magicui/cool-mode"

export default function CoolModeCustom() {
  return (
    <div className="relative justify-center">
      <CoolMode
        options={{
          particle: "https://avatars.githubusercontent.com/u/81306489",
        }}
      >
        <Button>Click Me!</Button>
      </CoolMode>
    </div>
  )
}
