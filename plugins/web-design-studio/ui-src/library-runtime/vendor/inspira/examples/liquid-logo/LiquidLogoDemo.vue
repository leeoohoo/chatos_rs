<script lang="ts" setup>
interface Props {
  imageUrl?: string;
  patternScale?: number;
  refraction?: number;
  edge?: number;
  patternBlur?: number;
  liquid?: number;
  speed?: number;
  showProcessing?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  imageUrl: "https://cdn.inspira-ui.com/images/apple-logo.svg",
  patternScale: 2,
  refraction: 0.015,
  edge: 0.4,
  patternBlur: 0.005,
  liquid: 0.07,
  speed: 0.3,
  showProcessing: true,
});
</script>

<template>
  <div class="flex h-96 w-full items-center justify-center">
    <LiquidLogo
      :image-url="props.imageUrl"
      :pattern-scale="props.patternScale"
      :refraction="props.refraction"
      :edge="props.edge"
      :pattern-blur="props.patternBlur"
      :liquid="props.liquid"
      :speed="props.speed"
      :show-processing="props.showProcessing"
    />
  </div>
</template>
