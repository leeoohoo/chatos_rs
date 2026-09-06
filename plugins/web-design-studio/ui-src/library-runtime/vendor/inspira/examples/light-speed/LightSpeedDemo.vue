<script setup lang="ts">
import { lightSpeedPresets } from "../../ui/light-speed";

interface Props {
  preset?: "one" | "two" | "three" | "four" | "five" | "six";
  speedUp?: number;
  fov?: number;
}

const props = withDefaults(defineProps<Props>(), {
  preset: "one",
  speedUp: 2,
  fov: 90,
});

const selectedPreset = computed(() => ({
  ...lightSpeedPresets[props.preset],
  speedUp: props.speedUp,
  fov: props.fov,
}));

function getName(key: string) {
  const names = {
    one: "Light 1",
    two: "Light 2",
    three: "Light 3",
    four: "Light 4",
    five: "Light 5",
    six: "Light 6",
  };

  return names[key];
}
</script>

<template>
  <div class="font-heading flex w-full flex-col gap-4">
    <div
      class="relative flex h-96 w-full flex-col items-center justify-center gap-4 overflow-clip rounded-xl border border-zinc-800"
    >
      <LightSpeed
        :key="`${props.preset}-${props.speedUp}-${props.fov}`"
        :effect-options="selectedPreset"
      />
      <div class="absolute top-6 text-2xl text-neutral-500">Click to speed up</div>
    </div>
    <p class="text-muted-foreground text-sm">{{ getName(props.preset) }}</p>
  </div>
</template>
