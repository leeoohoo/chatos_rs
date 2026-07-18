import { ContactShadows, useTexture } from '@react-three/drei';
import { useEffect, useState } from 'react';
import { DoubleSide, SRGBColorSpace } from 'three';

interface CatProps {
  onPet: () => void;
}

const CAT_DEPTH_LAYERS = [-0.024, -0.012, 0, 0.012, 0.024];

export function Cat({ onPet }: CatProps) {
  const [hovered, setHovered] = useState(false);
  const texture = useTexture('/assets/cat-exotic-transparent.png');

  useEffect(() => {
    texture.colorSpace = SRGBColorSpace;
    texture.needsUpdate = true;
  }, [texture]);

  return (
    <group position={[-1.2, 1.455, -0.08]} scale={hovered ? 1.025 : 1}>
      <ContactShadows
        position={[0, -0.398, 0.02]}
        scale={1.28}
        opacity={0.44}
        blur={2.8}
        far={0.72}
        frames={1}
        resolution={256}
        color="#241711"
      />
      <group
        onClick={(event) => {
          event.stopPropagation();
          onPet();
        }}
        onPointerEnter={(event) => {
          event.stopPropagation();
          setHovered(true);
          document.body.style.cursor = 'pointer';
        }}
        onPointerLeave={() => {
          setHovered(false);
          document.body.style.cursor = 'default';
        }}
      >
        {CAT_DEPTH_LAYERS.map((depth, index) => (
          <mesh key={depth} position={[0, 0, depth]} castShadow={index === CAT_DEPTH_LAYERS.length - 1}>
            <planeGeometry args={[1.08, 0.85]} />
            <meshStandardMaterial
              map={texture}
              transparent
              alphaTest={0.16}
              alphaToCoverage
              roughness={0.96}
              metalness={0}
              side={DoubleSide}
            />
          </mesh>
        ))}
      </group>
    </group>
  );
}
