<script setup lang="ts">
interface Props {
  initialSliderPercentage?: number;
  slideMode?: "hover" | "drag";
  showHandlebar?: boolean;
  autoplay?: boolean;
  autoplayDuration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  initialSliderPercentage: 50,
  slideMode: "hover",
  showHandlebar: true,
  autoplay: false,
  autoplayDuration: 5000,
});
</script>

<template>
  <div class="flex w-full justify-center">
    <div
      class="rounded-3xl border border-neutral-200 bg-neutral-100 p-4 dark:border-neutral-800 dark:bg-neutral-900"
    >
      <Compare
        first-image="https://images.vscodethemes.com/antfu.theme-vitesse/vitesse-light-soft-js-preview-4JM.svg"
        second-image="https://images.vscodethemes.com/antfu.theme-vitesse/vitesse-dark-soft-js-preview-4JM.svg"
        first-content-class="object-cover object-top-left rounded-xl overflow-hidden"
        second-content-class="object-cover object-top-left rounded-xl overflow-hidden"
        class="h-[250px] w-[200px] md:h-96 md:w-[500px]"
        :initial-slider-percentage="props.initialSliderPercentage"
        :slide-mode="props.slideMode"
        :show-handlebar="props.showHandlebar"
        :autoplay="props.autoplay"
        :autoplay-duration="props.autoplayDuration"
      />
    </div>
  </div>
</template>
