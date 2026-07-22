import { useTexture } from '@react-three/drei';
import { useFrame } from '@react-three/fiber';
import { useEffect } from 'react';
import { MathUtils, SRGBColorSpace } from 'three';
import type { TimeMode } from '../types';

interface WindowViewProps {
  timeMode: TimeMode;
}

export function WindowView({ timeMode }: WindowViewProps) {
  const [dayTexture, sunsetTexture, nightTexture] = useTexture([
    '/assets/window-day.jpg',
    '/assets/window-sunset.jpg',
    '/assets/window-night.jpg',
  ]);

  useEffect(() => {
    [dayTexture, sunsetTexture, nightTexture].forEach((texture) => {
      texture.colorSpace = SRGBColorSpace;
      texture.repeat.set(0.94, 0.79);
      texture.offset.set(0.03, 0.105);
      texture.needsUpdate = true;
    });
  }, [dayTexture, nightTexture, sunsetTexture]);

  useFrame(({ camera }) => {
    const targetOffsetX = 0.03 + MathUtils.clamp(camera.position.x * 0.004, -0.012, 0.012);
    [dayTexture, sunsetTexture, nightTexture].forEach((texture) => {
      texture.offset.x = MathUtils.lerp(texture.offset.x, targetOffsetX, 0.075);
    });
  });

  const texture = timeMode === 'day'
    ? dayTexture
    : timeMode === 'sunset'
      ? sunsetTexture
      : nightTexture;

  return (
    <group position={[0, 2.58, -3.28]}>
      <mesh position={[0, 0, -0.05]}>
        <planeGeometry args={[4.38, 2.34]} />
        <meshBasicMaterial map={texture} />
      </mesh>

      <mesh position={[0, 0, 0.015]}>
        <planeGeometry args={[4.38, 2.34]} />
        <meshPhysicalMaterial
          color={timeMode === 'night' ? '#8fa7c5' : '#d8edf4'}
          transparent
          opacity={timeMode === 'night' ? 0.055 : 0.085}
          roughness={0.08}
          metalness={0}
          clearcoat={1}
          clearcoatRoughness={0.08}
          envMapIntensity={0.72}
          ior={1.45}
          reflectivity={0.36}
        />
      </mesh>

      <mesh position={[0, 0, 0.11]} castShadow>
        <boxGeometry args={[0.085, 2.42, 0.16]} />
        <meshStandardMaterial color="#d7d1c8" roughness={0.48} />
      </mesh>
      <mesh position={[0, 0, 0.11]} castShadow>
        <boxGeometry args={[4.46, 0.085, 0.16]} />
        <meshStandardMaterial color="#d7d1c8" roughness={0.48} />
      </mesh>
      <mesh position={[0, 1.21, 0.1]} castShadow>
        <boxGeometry args={[4.62, 0.17, 0.24]} />
        <meshStandardMaterial color="#c9c1b7" roughness={0.56} />
      </mesh>
      <mesh position={[0, -1.21, 0.1]} castShadow>
        <boxGeometry args={[4.62, 0.17, 0.24]} />
        <meshStandardMaterial color="#c9c1b7" roughness={0.56} />
      </mesh>
      <mesh position={[-2.31, 0, 0.1]} castShadow>
        <boxGeometry args={[0.17, 2.58, 0.24]} />
        <meshStandardMaterial color="#c9c1b7" roughness={0.56} />
      </mesh>
      <mesh position={[2.31, 0, 0.1]} castShadow>
        <boxGeometry args={[0.17, 2.58, 0.24]} />
        <meshStandardMaterial color="#c9c1b7" roughness={0.56} />
      </mesh>

      <mesh position={[0, -1.39, 0.22]} castShadow receiveShadow>
        <boxGeometry args={[4.82, 0.14, 0.56]} />
        <meshStandardMaterial color="#bcb3a7" roughness={0.62} />
      </mesh>
    </group>
  );
}
