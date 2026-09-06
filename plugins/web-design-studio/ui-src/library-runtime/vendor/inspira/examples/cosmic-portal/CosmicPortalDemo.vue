<script lang="ts" setup>
interface Props {
  portalComplexity?: number;
  crystalCount?: number;
  primaryColor?: string;
  secondaryColor?: string;
  accentColor?: string;
  vortexColor?: string;
  rotationSpeed?: number;
  bloomStrength?: number;
  bloomRadius?: number;
  bloomThreshold?: number;
  dimensionShift?: number;
}

const props = withDefaults(defineProps<Props>(), {
  portalComplexity: 4,
  crystalCount: 12,
  primaryColor: "#9b59b6",
  secondaryColor: "#3498db",
  accentColor: "#e74c3c",
  vortexColor: "#2ecc71",
  rotationSpeed: 0.3,
  bloomStrength: 1.2,
  bloomRadius: 0.7,
  bloomThreshold: 0.2,
  dimensionShift: 4,
});

const cosmicPortal = useTemplateRef("cosmicPortal");

function activatePortal() {
  if (!cosmicPortal.value) return;
  cosmicPortal.value.activatePortal();
}

function shiftDimensions() {
  if (!cosmicPortal.value) return;
  cosmicPortal.value.shiftDimensions();
}
</script>

<template>
  <div class="flex h-112 w-full flex-col gap-4 overflow-clip">
    <CosmicPortal
      ref="cosmicPortal"
      container-class="rounded-lg"
      :portal-complexity="props.portalComplexity"
      :crystal-count="props.crystalCount"
      :primary-color="props.primaryColor"
      :secondary-color="props.secondaryColor"
      :accent-color="props.accentColor"
      :vortex-color="props.vortexColor"
      :rotation-speed="props.rotationSpeed"
      :bloom-strength="props.bloomStrength"
      :bloom-radius="props.bloomRadius"
      :bloom-threshold="props.bloomThreshold"
      :dimension-shift="props.dimensionShift"
    />
    <div class="flex flex-row items-center justify-center gap-4">
      <UiButton @click="activatePortal"> Activate Portal </UiButton>
      <UiButton @click="shiftDimensions"> Shift Dimensions </UiButton>
    </div>
  </div>
</template>
