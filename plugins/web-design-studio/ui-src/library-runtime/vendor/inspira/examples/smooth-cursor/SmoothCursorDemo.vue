<script lang="ts" setup>
interface Props {
  damping?: number;
  stiffness?: number;
  mass?: number;
  restDelta?: number;
}

const props = withDefaults(defineProps<Props>(), {
  damping: 45,
  stiffness: 400,
  mass: 1,
  restDelta: 0.001,
});

const springConfig = computed(() => ({
  damping: props.damping,
  stiffness: props.stiffness,
  mass: props.mass,
  restDelta: props.restDelta,
}));
</script>

<template>
  <div class="flex items-center justify-center text-xl">
    <span class="hidden md:block">Move your mouse around</span>
    <span class="block md:hidden">Tap anywhere to see the cursor</span>
    <SmoothCursor :spring-config="springConfig" />
  </div>
</template>
