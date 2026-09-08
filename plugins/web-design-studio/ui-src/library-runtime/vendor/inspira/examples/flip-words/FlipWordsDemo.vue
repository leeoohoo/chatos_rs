<script setup lang="ts">
interface Props {
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  duration: 3000,
});
</script>

<template>
  <div class="flex h-160 items-center justify-center px-4">
    <div class="mx-auto text-4xl font-normal text-neutral-600 dark:text-neutral-400">
      Vuejs: The
      <FlipWords
        :words="['Progressive', 'Approachable', 'Performant', 'Versatile']"
        :duration="props.duration"
        class="text-primary! text-4xl"
      />
      <div>
        JavaScript Framework for building <br />
        Modern Web Apps
      </div>
    </div>
  </div>
</template>
