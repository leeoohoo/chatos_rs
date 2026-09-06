<script lang="ts" setup>
interface Props {
  base?: number;
  squareColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  base: 15,
  squareColor: "#00C16A",
});
</script>

<template>
  <ClientOnly>
    <Tetris
      class="h-[500px] w-full mask-[radial-gradient(450px_circle_at_center,#00C16A,transparent)]"
      :base="props.base"
      :square-color="props.squareColor"
    />
  </ClientOnly>
</template>
