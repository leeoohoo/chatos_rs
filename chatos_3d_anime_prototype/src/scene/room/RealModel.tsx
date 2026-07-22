import { useGLTF } from '@react-three/drei';
import { useEffect, useMemo } from 'react';
import type { Mesh, Object3D } from 'three';

export function RealModel({
  url,
  position,
  rotation,
  scale = 1,
}: {
  url: string;
  position?: [number, number, number];
  rotation?: [number, number, number];
  scale?: number | [number, number, number];
}) {
  const { scene } = useGLTF(url);
  const model = useMemo(() => scene.clone(true), [scene]);

  useEffect(() => {
    model.traverse((child: Object3D) => {
      const mesh = child as Mesh;
      if (!mesh.isMesh) return;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
    });
  }, [model]);

  return <primitive object={model} position={position} rotation={rotation} scale={scale} />;
}
