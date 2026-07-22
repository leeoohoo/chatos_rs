import { Html, RoundedBox } from '@react-three/drei';
import { useEffect, useState, type ReactNode } from 'react';
import { Color } from 'three';
import type { DemoProject, DemoTask, TimeMode } from '../../types';
import { RealModel } from './RealModel';

const PROJECT_BOOKS_PER_PAGE = 6;

export function ProjectBookshelf({
  projects,
  focused,
  onFocus,
  onProjectSelect,
}: {
  projects: DemoProject[];
  focused: boolean;
  onFocus: () => void;
  onProjectSelect: (project: DemoProject) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [page, setPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(projects.length / PROJECT_BOOKS_PER_PAGE));
  const visibleProjects = projects.slice(page * PROJECT_BOOKS_PER_PAGE, (page + 1) * PROJECT_BOOKS_PER_PAGE);
  useEffect(() => {
    setPage((current) => Math.min(current, pageCount - 1));
  }, [pageCount]);

  return (
    <group
      position={[-5.5, 0.01, -0.55]}
      rotation={[0, Math.PI / 2, 0]}
      onClick={(event) => {
        event.stopPropagation();
        onFocus();
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
      <RealModel
        url="/assets/models/polyhaven/wooden_bookshelf_worn/wooden_bookshelf_worn.gltf"
        scale={1.55}
      />
      <mesh position={[0, 1.58, 0.08]}>
        <boxGeometry args={[2.18, 3.2, 0.86]} />
        <meshBasicMaterial transparent opacity={0} depthWrite={false} colorWrite={false} />
      </mesh>

      <RealModel
        url="/assets/models/polyhaven/book_encyclopedia_set_01/book_encyclopedia_set_01.gltf"
        position={[-0.68, 2.45, 0.39]}
        rotation={[0, 0.05, 0]}
        scale={1.45}
      />
      <RealModel
        url="/assets/models/polyhaven/book_encyclopedia_set_01/book_encyclopedia_set_01.gltf"
        position={[-0.75, 0.28, 0.39]}
        rotation={[0, -0.03, 0]}
        scale={1.4}
      />

      {visibleProjects.map((project, index) => {
        const column = index % 3;
        const row = Math.floor(index / 3);
        return (
          <group
            key={project.id}
            position={[-0.52 + column * 0.52, 1.22 + row * 0.76, 0.47 + (column === 1 ? 0.025 : 0)]}
            rotation={[0, 0, column === 0 ? -0.035 : column === 2 ? 0.035 : 0]}
            onClick={(event) => {
              event.stopPropagation();
              onProjectSelect(project);
            }}
          >
            <RoundedBox args={[0.42, 0.76, 0.2]} radius={0.035} smoothness={3} castShadow>
              <meshStandardMaterial
                color={hovered || focused ? project.accent : new Color(project.accent).multiplyScalar(0.78)}
                roughness={0.8}
              />
            </RoundedBox>
            <mesh position={[-0.17, 0, 0.115]} castShadow>
              <boxGeometry args={[0.035, 0.72, 0.012]} />
              <meshStandardMaterial color="#d2b37c" metalness={0.14} roughness={0.56} />
            </mesh>
            <mesh position={[0, 0.2, 0.112]}>
              <boxGeometry args={[0.28, 0.12, 0.016]} />
              <meshStandardMaterial color="#e7dcc5" roughness={0.88} />
            </mesh>
            <Html transform center distanceFactor={1.9} position={[0, -0.08, 0.122]} style={{ pointerEvents: 'auto' }}>
              <button
                type="button"
                className="bookshelf-project-spine"
                onPointerDown={(event) => event.stopPropagation()}
                onClick={(event) => {
                  event.stopPropagation();
                  onProjectSelect(project);
                }}
              >
                <b>{project.name}</b>
                <span>{project.status === 'running' ? '进行中' : project.status === 'planning' ? '规划中' : '已归档'}</span>
              </button>
            </Html>
          </group>
        );
      })}

      <Html center distanceFactor={3.05} position={[0, 0.58, 0.5]} style={{ pointerEvents: 'auto' }}>
        <div
          className={focused ? 'bookshelf-pagination is-focused' : 'bookshelf-pagination'}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
        >
          <button type="button" aria-label="上一页项目" disabled={page === 0} onClick={() => setPage((current) => Math.max(0, current - 1))}>‹</button>
          <span><b>用户项目 · 每页 6 册</b><small>{page + 1} / {pageCount} · 共 {projects.length} 册</small></span>
          <button type="button" aria-label="下一页项目" disabled={page >= pageCount - 1} onClick={() => setPage((current) => Math.min(pageCount - 1, current + 1))}>›</button>
        </div>
      </Html>
    </group>
  );
}

const TASK_COLORS: Record<DemoTask['status'], string> = {
  doing: '#62b4ff',
  todo: '#a9b6c7',
  blocked: '#ff869d',
  done: '#73d9aa',
};

export function TaskWallDisplay({
  tasks,
  timeMode,
  onOpen,
  focused,
  content,
}: {
  tasks: DemoTask[];
  timeMode: TimeMode;
  onOpen: () => void;
  focused: boolean;
  content: ReactNode;
}) {
  const [hovered, setHovered] = useState(false);

  return (
    <group
      position={[5.78, 2.32, -0.42]}
      rotation={[0, -Math.PI / 2, 0]}
      onClick={(event) => {
        event.stopPropagation();
        if (!focused) onOpen();
      }}
      onPointerEnter={(event) => {
        event.stopPropagation();
        if (!focused) {
          setHovered(true);
          document.body.style.cursor = 'pointer';
        }
      }}
      onPointerLeave={() => {
        setHovered(false);
        document.body.style.cursor = 'default';
      }}
    >
      <RoundedBox args={[3.98, 2.38, 0.1]} radius={0.055} smoothness={3} position={[0, 0, -0.035]} castShadow>
        <meshStandardMaterial color="#171c20" metalness={0.68} roughness={0.25} />
      </RoundedBox>
      <mesh position={[0, 0, 0.025]}>
        <planeGeometry args={[3.78, 2.18]} />
        <meshPhysicalMaterial
          color="#081927"
          emissive={hovered || focused ? '#4c84b2' : '#254867'}
          emissiveIntensity={focused ? 0.38 : hovered ? 0.3 : 0.16}
          transparent
          opacity={focused ? 0.9 : 0.72}
          roughness={0.16}
          clearcoat={0.9}
          clearcoatRoughness={0.1}
          toneMapped={false}
        />
      </mesh>

      {focused ? (
        <Html center distanceFactor={2.5} position={[0, 0, 0.07]} style={{ pointerEvents: 'auto' }}>
          <div onPointerDown={(event) => event.stopPropagation()} onClick={(event) => event.stopPropagation()}>
            {content}
          </div>
        </Html>
      ) : (
        <Html transform center distanceFactor={5.15} position={[0, 0, 0.07]} style={{ pointerEvents: 'none' }}>
          {tasks.length > 0 ? (
            <div className="task-wall-preview">
              <div className="task-wall-preview__header">
                <b>LIVE TASKS</b>
                <span>{tasks.length} running</span>
              </div>
              {tasks.slice(0, 3).map((task) => (
                <div className="task-wall-preview__task" key={task.id}>
                  <i style={{ background: TASK_COLORS[task.status] }} />
                  <span>{task.title}</span>
                  <em>{task.progress}%</em>
                </div>
              ))}
            </div>
          ) : (
            <div className={`task-wall-preview is-empty is-${timeMode}`}>
              <img src={`/assets/window-${timeMode}.jpg`} alt="" />
              <div className="task-wall-preview__empty-copy">
                <b>WORKSPACE STANDBY</b>
                <span>暂无正在执行的任务</span>
              </div>
            </div>
          )}
        </Html>
      )}

      {[[-1.95, 1.2], [1.95, 1.2], [-1.95, -1.2], [1.95, -1.2]].map(([x, y], index) => (
        <group key={index} position={[x, y, 0.08]}>
          <mesh position={[x < 0 ? 0.11 : -0.11, 0, 0]}>
            <boxGeometry args={[0.22, 0.035, 0.025]} />
            <meshBasicMaterial color="#9ed5ff" transparent opacity={0.7} />
          </mesh>
          <mesh position={[0, y < 0 ? 0.11 : -0.11, 0]}>
            <boxGeometry args={[0.035, 0.22, 0.025]} />
            <meshBasicMaterial color="#9ed5ff" transparent opacity={0.7} />
          </mesh>
        </group>
      ))}
    </group>
  );
}
