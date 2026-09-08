<script setup lang="ts">
import { ORBIT_DIRECTION } from "../../ui/orbit";

interface Props {
  duration?: number;
  innerRadius?: number;
  outerRadius?: number;
  showPath?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  duration: 20,
  innerRadius: 80,
  outerRadius: 190,
  showPath: true,
});
</script>

<template>
  <div
    class="bg-background relative flex h-125 w-full flex-col items-center justify-center overflow-hidden rounded-lg border md:shadow-xl"
  >
    <span class="pointer-events-none text-center text-8xl leading-none font-semibold"> 🌍 </span>

    <!-- Inner Circles -->
    <Orbit
      class="size-7.5 items-center justify-center border-none bg-transparent"
      :duration="props.duration"
      :delay="20"
      :radius="props.innerRadius"
      :direction="ORBIT_DIRECTION.CounterClockwise"
    >
      <Icon
        name="logos:whatsapp-icon"
        size="24"
      />
    </Orbit>
    <Orbit
      class="size-7.5 items-center justify-center border-none bg-transparent"
      :duration="props.duration"
      :delay="10"
      :radius="props.innerRadius"
      :path="props.showPath"
      :direction="ORBIT_DIRECTION.CounterClockwise"
    >
      <Icon
        name="logos:notion-icon"
        size="24"
      />
    </Orbit>

    <!-- Outer Circles (reverse) -->
    <Orbit
      class="size-12.5 items-center justify-center border-none bg-transparent"
      :radius="props.outerRadius"
      :duration="props.duration"
      :path="props.showPath"
    >
      <Icon
        name="logos:google-drive"
        size="50"
      />
    </Orbit>
    <Orbit
      class="size-12.5 items-center justify-center border-none bg-transparent"
      :radius="props.outerRadius"
      :duration="props.duration"
      :delay="200"
      :direction="ORBIT_DIRECTION.CounterClockwise"
    >
      <Icon
        name="mdi:github"
        size="50"
      />
    </Orbit>
    <Orbit
      class="items-center justify-center border-none bg-transparent text-4xl"
      :radius="140"
      :delay="4"
    >
      🌕
    </Orbit>
  </div>
</template>
