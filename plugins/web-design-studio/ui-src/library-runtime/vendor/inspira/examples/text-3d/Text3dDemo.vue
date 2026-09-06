<script setup lang="ts">
interface Props {
  textColor?: string;
  strokeColor?: string;
  shadowColor?: string;
  strokeSize?: number;
  shadow1Size?: number;
  shadow2Size?: number;
  animate?: boolean;
  animationDuration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  textColor: "white",
  strokeColor: "black",
  shadowColor: "yellow",
  strokeSize: 20,
  shadow1Size: 7,
  shadow2Size: 10,
  animate: true,
  animationDuration: 1500,
});
</script>

<template>
  <div
    class="font-heading flex flex-col items-center justify-center overflow-hidden bg-black px-4 py-16"
  >
    <Text3d
      class="text-8xl font-bold max-md:text-7xl"
      v-bind="props"
    >
      3D
    </Text3d>
    <Text3d
      class="text-8xl font-black max-md:text-7xl"
      v-bind="props"
    >
      IS
    </Text3d>
    <Text3d
      class="text-8xl font-bold max-md:text-7xl"
      v-bind="props"
    >
      AWESOME
    </Text3d>
  </div>
</template>
