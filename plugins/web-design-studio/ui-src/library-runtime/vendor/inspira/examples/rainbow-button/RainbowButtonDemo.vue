<script setup lang="ts">
interface Props {
  speed?: number;
}

const props = withDefaults(defineProps<Props>(), {
  speed: 2,
});
</script>

<template>
  <div class="flex min-h-64 items-center justify-center">
    <RainbowButton :speed="props.speed">Rainbow Button</RainbowButton>
  </div>
</template>
