<script setup lang="ts">
interface Props {
  width?: number;
  height?: number;
  columns?: number;
  rows?: number;
  text?: string;
}

const props = withDefaults(defineProps<Props>(), {
  width: 40,
  height: 40,
  columns: 24,
  rows: 24,
  text: "Interactive Grid Pattern",
});
</script>

<template>
  <div class="relative grid h-[500px] place-content-center overflow-hidden">
    <p
      class="z-10 text-center text-5xl font-medium tracking-tighter whitespace-pre-wrap text-black dark:text-white"
    >
      {{ props.text }}
    </p>

    <InteractiveGridPattern
      class="inset-0 h-[200%] skew-y-12 mask-[radial-gradient(350px_circle_at_center,white,transparent)]"
      :width="props.width"
      :height="props.height"
      :squares="[props.columns, props.rows]"
    />
  </div>
</template>
