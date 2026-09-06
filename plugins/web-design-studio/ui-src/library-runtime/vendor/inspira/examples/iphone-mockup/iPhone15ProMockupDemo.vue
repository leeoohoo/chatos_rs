<!-- eslint-disable check-file/filename-naming-convention -->
<script setup lang="ts">
interface Props {
  src?: string;
  width?: number;
  height?: number;
}

const props = withDefaults(defineProps<Props>(), {
  src: "https://cdn.inspira-ui.com/images/inspira-ss-phone.png",
  width: 433,
  height: 882,
});
</script>

<template>
  <div class="relative flex w-full flex-col items-center justify-center p-8">
    <iPhone15ProMockup
      :src="props.src"
      :width="props.width"
      :height="props.height"
      class="size-full max-w-sm"
    />
  </div>
</template>
