<script setup lang="ts">
interface Props {
  color?: string;
  baseThickness?: number;
  speedMultiplier?: number;
  maxAge?: number;
  enableFade?: boolean;
  enableShaderEffect?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  color: "#ffffff",
  baseThickness: 30,
  speedMultiplier: 0.5,
  maxAge: 500,
  enableFade: false,
  enableShaderEffect: false,
});
</script>

<template>
  <div
    class="relative flex h-96 w-full flex-col items-center justify-center gap-4 overflow-clip rounded-xl"
  >
    <span class="absolute text-3xl opacity-50"> Hover here </span>
    <TailedCursor
      :base-thickness="props.baseThickness"
      :colors="[props.color]"
      :speed-multiplier="props.speedMultiplier"
      :max-age="props.maxAge"
      :enable-fade="props.enableFade"
      :enable-shader-effect="props.enableShaderEffect"
    />
  </div>
</template>
