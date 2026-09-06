<script setup lang="ts">
interface Props {
  particleCount?: number;
  rangeY?: number;
  baseHue?: number;
  baseSpeed?: number;
  rangeSpeed?: number;
  baseRadius?: number;
  rangeRadius?: number;
  backgroundColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  particleCount: 700,
  rangeY: 100,
  baseHue: 220,
  baseSpeed: 0,
  rangeSpeed: 1.5,
  baseRadius: 1,
  rangeRadius: 2,
  backgroundColor: "#000000",
});
</script>

<template>
  <div class="mx-auto h-120 w-[calc(100%-4rem)] overflow-hidden rounded-md">
    <Vortex
      :particle-count="props.particleCount"
      :range-y="props.rangeY"
      :base-hue="props.baseHue"
      :base-speed="props.baseSpeed"
      :range-speed="props.rangeSpeed"
      :base-radius="props.baseRadius"
      :range-radius="props.rangeRadius"
      :background-color="props.backgroundColor"
      class="flex size-full flex-col items-center justify-center px-2 py-4 md:px-10"
    >
      <h2 class="text-center text-2xl font-bold text-white md:text-6xl">The hell is this?</h2>
      <p class="mt-6 max-w-xl text-center text-sm text-white md:text-2xl">
        This is chemical burn. It&apos;ll hurt more than you&apos;ve ever been burned and
        you&apos;ll have a scar.
      </p>
      <div class="mt-6 flex flex-col items-center gap-4 sm:flex-row">
        <button
          class="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700"
        >
          Order now
        </button>
        <button
          class="rounded-md bg-white px-4 py-2 text-sm font-medium text-black transition-colors hover:bg-gray-100"
        >
          Watch trailer
        </button>
      </div>
    </Vortex>
  </div>
</template>
