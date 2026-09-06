<script setup lang="ts">
interface Props {
  magnification?: number;
  distance?: number;
  direction?: "top" | "middle" | "bottom";
  orientation?: "horizontal" | "vertical";
}

const props = withDefaults(defineProps<Props>(), {
  magnification: 60,
  distance: 140,
  direction: "middle",
  orientation: "horizontal",
});
</script>

<template>
  <Dock
    class="mb-6"
    :magnification="props.magnification"
    :distance="props.distance"
    :direction="props.direction"
    :orientation="props.orientation"
  >
    <DockIcon>
      <Icon
        name="mdi:github"
        class="size-full"
      />
    </DockIcon>
    <DockSeparator />
    <DockIcon>
      <Icon
        name="logos:google-drive"
        class="size-full"
      />
    </DockIcon>
    <DockIcon>
      <Icon
        name="logos:notion-icon"
        class="size-full"
      />
    </DockIcon>
    <DockIcon>
      <Icon
        name="logos:whatsapp-icon"
        class="size-full"
      />
    </DockIcon>
  </Dock>
</template>
