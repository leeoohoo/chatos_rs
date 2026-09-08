<script setup lang="ts">
interface Props {
  text?: string;
  sparklesCount?: number;
  colors?: {
    first: string;
    second: string;
  };
}

const props = withDefaults(defineProps<Props>(), {
  text: "Inspira UI",
  sparklesCount: 10,
  colors: () => ({ first: "#9e7aff", second: "#fe8bbb" }),
});
</script>

<template>
  <div class="flex h-80 items-center justify-center">
    <SparklesText
      :text="props.text"
      :colors="props.colors"
      :sparkles-count="props.sparklesCount"
      class="my-8"
    />
  </div>
</template>
