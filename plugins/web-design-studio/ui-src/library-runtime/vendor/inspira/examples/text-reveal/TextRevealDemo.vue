<script setup lang="ts">
interface Props {
  text?: string;
  duration?: number;
  delay?: number;
  stagger?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "With Inspira UI, you can build beautiful websites powered by Vue and Nuxt in record time.",
  duration: 0.6,
  delay: 0.2,
  stagger: 0.1,
});
</script>

<template>
  <div class="flex items-center justify-center p-8">
    <TextReveal
      class="text-center text-4xl leading-relaxed font-semibold"
      :duration="props.duration"
      :delay="props.delay"
      :stagger="props.stagger"
    >
      {{ props.text }}
    </TextReveal>
  </div>
</template>
