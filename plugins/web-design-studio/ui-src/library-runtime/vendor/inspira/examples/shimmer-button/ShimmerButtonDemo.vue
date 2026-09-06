<script setup lang="ts">
interface Props {
  shimmerColor?: string;
  shimmerSize?: string;
  borderRadius?: string;
  shimmerDuration?: string;
  background?: string;
}

const props = withDefaults(defineProps<Props>(), {
  shimmerColor: "#ffffff",
  shimmerSize: "2px",
  borderRadius: "100px",
  shimmerDuration: "3s",
  background: "rgba(0, 0, 0, 1)",
});
</script>

<template>
  <div class="z-10 flex min-h-64 items-center justify-center">
    <ShimmerButton
      class="shadow-2xl"
      :shimmer-color="props.shimmerColor"
      :shimmer-size="props.shimmerSize"
      :border-radius="props.borderRadius"
      :shimmer-duration="props.shimmerDuration"
      :background="props.background"
    >
      <span
        class="text-center text-sm leading-none font-medium tracking-tight whitespace-pre-wrap text-white lg:text-lg dark:from-white dark:to-slate-900/10"
      >
        Cool Button
      </span>
    </ShimmerButton>
  </div>
</template>
