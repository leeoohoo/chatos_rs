<script setup lang="ts">
interface Props {
  friction?: number;
  trails?: number;
  size?: number;
  dampening?: number;
  tension?: number;
}

const props = withDefaults(defineProps<Props>(), {
  friction: 0.5,
  trails: 20,
  size: 50,
  dampening: 0.25,
  tension: 0.98,
});
</script>

<template>
  <div class="relative flex h-96 min-h-96 w-full items-center justify-center text-xl">
    <span class="hidden md:block">Move your mouse around</span>
    <span class="block md:hidden">Swipe anywhere to see the cursor</span>
    <ClientOnly>
      <SleekLineCursor
        :friction="props.friction"
        :trails="props.trails"
        :size="props.size"
        :dampening="props.dampening"
        :tension="props.tension"
      />
    </ClientOnly>
  </div>
</template>
