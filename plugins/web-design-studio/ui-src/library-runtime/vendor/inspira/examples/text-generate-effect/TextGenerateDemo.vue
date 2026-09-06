<script setup lang="ts">
interface Props {
  words?: string;
  filter?: boolean;
  duration?: number;
  delay?: number;
}

const props = withDefaults(defineProps<Props>(), {
  words:
    "Nuxt is an open source framework that makes web development intuitive and powerful. Create performant and production-grade full-stack web apps and websites with confidence.",
  filter: true,
  duration: 0.7,
  delay: 0,
});
</script>

<template>
  <div class="flex items-center justify-center py-12 max-lg:w-full md:flex-1">
    <TextGenerateEffect
      :words="props.words"
      :filter="props.filter"
      :duration="props.duration"
      :delay="props.delay"
    />
  </div>
</template>
