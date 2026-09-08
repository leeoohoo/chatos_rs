<script setup lang="ts">
interface Props {
  url?: string;
  src?: string;
  width?: number;
  height?: number;
}

const props = withDefaults(defineProps<Props>(), {
  url: "inspira-ui.com",
  src: "https://cdn.inspira-ui.com/images/inspira-ss.png",
  width: 1203,
  height: 753,
});
</script>

<template>
  <div class="relative p-4">
    <SafariMockup
      :url="props.url"
      :src="props.src"
      :width="props.width"
      :height="props.height"
      class="size-full"
    />
  </div>
</template>
