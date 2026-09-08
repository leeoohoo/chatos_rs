<script setup lang="ts">
interface Props {
  words?: string;
  delay?: number;
}

const props = withDefaults(defineProps<Props>(), {
  words: "Staggered Letter Pull Up",
  delay: 0.05,
});
</script>

<template>
  <LetterPullup
    :words="props.words"
    :delay="props.delay"
    class="text-black dark:text-white"
  />
</template>
