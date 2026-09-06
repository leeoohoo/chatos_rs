<script setup lang="ts">
interface Props {
  strokeColor?: string;
  numberOfLines?: number;
  numberOfDiscs?: number;
}

const props = withDefaults(defineProps<Props>(), {
  strokeColor: "#737373",
  numberOfLines: 50,
  numberOfDiscs: 50,
});
</script>

<template>
  <div class="relative h-96 w-full">
    <BlackHoleBackground
      class="absolute inset-0 flex items-center justify-center rounded-xl"
      :stroke-color="props.strokeColor"
      :number-of-lines="props.numberOfLines"
      :number-of-discs="props.numberOfDiscs"
    />
  </div>
</template>
