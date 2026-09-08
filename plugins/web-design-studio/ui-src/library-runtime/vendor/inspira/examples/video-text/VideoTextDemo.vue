<script setup lang="ts">
interface Props {
  text?: string;
  fontSize?: number;
  fontWeight?: number;
  autoPlay?: boolean;
  muted?: boolean;
  loop?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  text: "INSPIRA",
  fontSize: 20,
  fontWeight: 700,
  autoPlay: true,
  muted: true,
  loop: true,
});
</script>

<template>
  <div class="relative h-[200px] w-full overflow-hidden">
    <VideoText
      src="https://cdn.magicui.design/ocean-small.webm"
      :font-size="props.fontSize"
      :font-weight="props.fontWeight"
      :auto-play="props.autoPlay"
      :muted="props.muted"
      :loop="props.loop"
    >
      {{ props.text }}
    </VideoText>
  </div>
</template>
