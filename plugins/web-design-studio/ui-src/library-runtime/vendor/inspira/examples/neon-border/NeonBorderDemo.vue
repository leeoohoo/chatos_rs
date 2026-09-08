<script setup lang="ts">
interface Props {
  color1?: string;
  color2?: string;
  animationType?: "none" | "half" | "full";
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  color1: "#0496ff",
  color2: "#ff0a54",
  animationType: "half",
  duration: 6,
});
</script>

<template>
  <div class="flex h-96 w-full flex-col items-center justify-center gap-16">
    <NeonBorder
      animation-type="none"
      :color1="props.color1"
      :color2="props.color2"
      :duration="props.duration"
    >
      <input
        type="text"
        class="bg-background size-full rounded-lg px-4 text-sm"
        placeholder="No animation"
      />
    </NeonBorder>

    <NeonBorder
      :color1="props.color1"
      :color2="props.color2"
      :animation-type="props.animationType"
      :duration="props.duration"
    >
      <input
        type="text"
        class="bg-background size-full rounded-lg px-4 text-sm"
        placeholder="Half border animation"
      />
    </NeonBorder>

    <NeonBorder
      animation-type="full"
      :color1="props.color1"
      :color2="props.color2"
      :duration="props.duration"
    >
      <input
        type="text"
        class="bg-background size-full rounded-lg px-4 text-sm"
        placeholder="Full border animation"
      />
    </NeonBorder>
  </div>
</template>
