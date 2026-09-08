<script setup lang="ts">
interface Props {
  text?: string;
  duration?: number;
  radiantWidth?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "✨ Introducing Inspira UI",
  duration: 5,
  radiantWidth: 100,
});
</script>

<template>
  <RadiantText
    class="inline-flex items-center justify-center px-4 py-1 transition ease-out hover:text-neutral-600 hover:duration-300 hover:dark:text-neutral-400"
    :duration="props.duration"
    :radiant-width="props.radiantWidth"
  >
    <span class="text-3xl font-bold">{{ props.text }}</span>
  </RadiantText>
</template>
