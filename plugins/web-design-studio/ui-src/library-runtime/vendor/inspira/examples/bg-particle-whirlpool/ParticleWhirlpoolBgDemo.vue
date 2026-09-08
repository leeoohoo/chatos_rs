<script setup lang="ts">
interface Props {
  particleCount?: number;
  blur?: number;
}

const props = withDefaults(defineProps<Props>(), {
  particleCount: 2000,
  blur: 2,
});
</script>

<template>
  <ParticleWhirlpoolBg
    :particle-count="props.particleCount"
    :blur="props.blur"
    class="h-96"
  >
    <div
      class="font-heading flex size-full flex-col items-center justify-center text-5xl text-white"
    >
      Cool Background
    </div>
  </ParticleWhirlpoolBg>
</template>
