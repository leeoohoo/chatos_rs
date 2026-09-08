<script setup lang="ts">
interface Props {
  text?: string;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Making UI beautiful using Inspira UI.",
});
</script>

<template>
  <div class="z-10 flex min-h-64 w-full items-center justify-center rounded-lg">
    <TextScrollReveal :text="props.text" />
  </div>
</template>
