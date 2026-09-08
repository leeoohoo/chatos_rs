<script setup lang="ts">
interface Props {
  renderer?: "default" | "webgl";
  particleSize?: number;
  particleGap?: number;
  mouseForce?: number;
  gravity?: number;
  initPosition?: "random" | "top" | "left" | "bottom" | "right" | "misplaced" | "none";
  fadePosition?: "explode" | "top" | "left" | "bottom" | "right" | "random" | "none";
  responsiveWidth?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  renderer: "default",
  particleSize: 1,
  particleGap: 2,
  mouseForce: 20,
  gravity: 0.08,
  initPosition: "random",
  fadePosition: "explode",
  responsiveWidth: true,
});
</script>

<template>
  <div class="flex flex-col items-center justify-center">
    <ParticleImage
      image-src="https://cdn.inspira-ui.com/og-image.png"
      :renderer="props.renderer"
      :particle-size="String(props.particleSize)"
      :particle-gap="String(props.particleGap)"
      :mouse-force="String(props.mouseForce)"
      :gravity="String(props.gravity)"
      :init-position="props.initPosition"
      :fade-position="props.fadePosition"
      :responsive-width="props.responsiveWidth"
    />
  </div>
</template>
