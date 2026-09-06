<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    border?: "dashed" | "solid";
    density?: "compact" | "comfortable";
    showGrid?: boolean;
  }>(),
  {
    border: "dashed",
    density: "comfortable",
    showGrid: true,
  },
);

const uploadClass = computed(() => [
  "rounded-lg border border-neutral-200 dark:border-neutral-800",
  props.border === "dashed" ? "border-dashed" : "border-solid",
]);

const wrapperClass = computed(() =>
  props.density === "compact" ? "space-y-4 p-4" : "space-y-6 p-8",
);
</script>

<template>
  <div
    class="dark:bg-black"
    :class="wrapperClass"
  >
    <FileUpload :class="uploadClass">
      <FileUploadGrid v-if="props.showGrid" />
    </FileUpload>
  </div>
</template>
