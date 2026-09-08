<script setup lang="ts">
interface Props {
  blur?: number;
}

const props = withDefaults(defineProps<Props>(), {
  blur: 0,
});
</script>

<template>
  <BubblesBg
    class="h-96 w-full"
    :blur="props.blur"
  />
</template>
