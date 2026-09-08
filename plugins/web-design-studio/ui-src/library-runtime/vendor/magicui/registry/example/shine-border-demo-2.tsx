// @ts-nocheck
"use client"

import { useTheme } from "next-themes"

import { Button } from "@magic/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@magic/components/ui/card"
import { Input } from "@magic/components/ui/input"
import { Label } from "@magic/components/ui/label"
import { ShineBorder } from "@magic/registry/magicui/shine-border"

export default function ShineBorderDemo2() {
  const theme = useTheme()
  return (
    <Card className="relative overflow-hidden">
      <ShineBorder shineColor={theme.theme === "dark" ? "white" : "black"} />
      <CardHeader>
        <CardTitle>Login</CardTitle>
        <CardDescription>
          Enter your credentials to access your account
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form>
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="email">Email</Label>
              <Input id="email" type="email" placeholder="name@example.com" />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input id="password" type="password" />
            </div>
          </div>
        </form>
      </CardContent>
      <CardFooter>
        <Button className="w-full">Sign In</Button>
      </CardFooter>
    </Card>
  )
}
