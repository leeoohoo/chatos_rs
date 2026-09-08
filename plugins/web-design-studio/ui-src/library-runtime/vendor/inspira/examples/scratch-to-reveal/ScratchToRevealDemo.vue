<script lang="ts" setup>
interface Props {
  width?: number;
  height?: number;
  minScratchPercentage?: number;
  color1?: string;
  color2?: string;
  color3?: string;
}

const props = withDefaults(defineProps<Props>(), {
  width: 250,
  height: 250,
  minScratchPercentage: 50,
  color1: "#A97CF8",
  color2: "#F38CB8",
  color3: "#FDCC92",
});

function handleComplete() {
  // when scratch is completed do something
}
</script>

<template>
  <ScratchToReveal
    :width="props.width"
    :height="props.height"
    :min-scratch-percentage="props.minScratchPercentage"
    class="mx-auto flex items-center justify-center overflow-hidden rounded-2xl border-2 bg-gray-100"
    :gradient-colors="[props.color1, props.color2, props.color3]"
    @complete="handleComplete"
  >
    <div class="text-8xl">🥳</div>
  </ScratchToReveal>
</template>
