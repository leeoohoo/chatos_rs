<script lang="ts" setup>
interface Props {
  perspective?: number;
  beamsPerSide?: number;
  beamSize?: number;
  beamDelayMax?: number;
  beamDuration?: number;
  gridColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  perspective: 100,
  beamsPerSide: 3,
  beamSize: 5,
  beamDelayMax: 3,
  beamDuration: 3,
  gridColor: "hsl(var(--border))",
});
</script>

<template>
  <div class="w-full">
    <WarpBackground
      :perspective="props.perspective"
      :beams-per-side="props.beamsPerSide"
      :beam-size="props.beamSize"
      :beam-delay-max="props.beamDelayMax"
      :beam-duration="props.beamDuration"
      :grid-color="props.gridColor"
    >
      <div class="bg-card mx-auto w-72 rounded-lg border shadow-sm">
        <div class="flex flex-col gap-2 p-4">
          <h3 class="text-xl leading-none font-semibold tracking-tight">
            Congratulations on Your Promotion!
          </h3>
          <p class="text-sm text-gray-500 dark:text-gray-400">
            Your hard work and dedication have paid off. We&apos;re thrilled to see you take this
            next step in your career. Keep up the fantastic work!
          </p>
        </div>
      </div>
    </WarpBackground>
  </div>
</template>
