<script setup lang="ts">
interface Props {
  text?: string;
  delay?: number;
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Lamp Effect",
  delay: 0.5,
  duration: 0.8,
});
</script>

<template>
  <LampEffect
    :delay="props.delay"
    :duration="props.duration"
  >
    <span class="font-heading text-6xl"> {{ props.text }} </span>
  </LampEffect>
</template>
