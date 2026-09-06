<script setup lang="ts">
interface Props {
  rippleColor?: string;
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  rippleColor: "#add8e6",
  duration: 600,
});
</script>

<template>
  <div class="grid place-content-center p-8">
    <RippleButton
      :ripple-color="props.rippleColor"
      :duration="props.duration"
    >
      Click me!
    </RippleButton>
  </div>
</template>
