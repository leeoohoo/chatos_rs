import { useEffect, useMemo, useState } from "react";
import { apiClient } from "./api/client";
import { AppShell } from "./components/layout/AppShell";
import { ConnectionModal } from "./components/workbench/ConnectionModal";
import { ConnectionSidebar } from "./components/workbench/ConnectionSidebar";
import { InspectorTabs } from "./components/workbench/InspectorTabs";
import {
  detailToEditorValue,
  toCreatePayload,
  toUpdatePayload,
  type ConnectionEditorValue
} from "./components/workbench/connectionFormAdapter";
import { SchemaBrowserPanel } from "./components/workbench/SchemaBrowserPanel";
import { supportsDetailByNodeIdForDb } from "./components/explorer/metadataNodeUtils";
import type {
  DataSourceListItem,
  DbType,
  DbTypeDescriptor,
  MetadataNode,
  ObjectDetailResponse,
  QueryExecuteResponse
} from "./types/models";

const DATABASE_PAGE_SIZE = 500;
const NODE_PAGE_SIZE = 300;

export default function App() {
  const [dbTypes, setDbTypes] = useState<DbTypeDescriptor[]>([]);
  const [datasources, setDatasources] = useState<DataSourceListItem[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [activeDbType, setActiveDbType] = useState<DbType | null>(null);

  const [loadingDatabases, setLoadingDatabases] = useState(false);
  const [databases, setDatabases] = useState<Array<{ name: string }>>([]);
  const [selectedDatabase, setSelectedDatabase] = useState<string | null>(null);

  const [loadingObjects, setLoadingObjects] = useState(false);
  const [objects, setObjects] = useState<MetadataNode[]>([]);
  const [objectParentStack, setObjectParentStack] = useState<string[]>([]);
  const [objectPath, setObjectPath] = useState<string[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  const [detail, setDetail] = useState<ObjectDetailResponse | null>(null);
  const [sql, setSql] = useState("SELECT 1;");
  const [sqlLoading, setSqlLoading] = useState(false);
  const [sqlResult, setSqlResult] = useState<QueryExecuteResponse | null>(null);
  const [inspectorTab, setInspectorTab] = useState<"detail" | "sql">("detail");

  const [modalOpen, setModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<"create" | "edit">("create");
  const [modalSubmitting, setModalSubmitting] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState<ConnectionEditorValue | null>(null);

  const [error, setError] = useState<string | null>(null);

  const activeConnection = useMemo(
    () => datasources.find((item) => item.id === activeId) ?? null,
    [datasources, activeId]
  );

  useEffect(() => {
    void bootstrap();
  }, []);

  async function bootstrap() {
    try {
      setError(null);
      const [dbTypeResponse, datasourceResponse] = await Promise.all([
        apiClient.getDbTypes(),
        apiClient.listDatasources()
      ]);
      setDbTypes(dbTypeResponse.items);
      setDatasources(datasourceResponse.items);
      if (datasourceResponse.items.length > 0) {
        await selectDatasource(datasourceResponse.items[0].id);
      } else {
        clearWorkspace();
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Bootstrap failed");
    }
  }

  function clearWorkspace() {
    setActiveId(null);
    setActiveDbType(null);
    setDatabases([]);
    setSelectedDatabase(null);
    setObjects([]);
    setObjectParentStack([]);
    setObjectPath([]);
    setSelectedNodeId(null);
    setDetail(null);
    setSqlResult(null);
    setInspectorTab("detail");
  }

  async function refreshDatasourceList() {
    const response = await apiClient.listDatasources();
    setDatasources(response.items);
    return response.items;
  }

  async function selectDatasource(id: string, preferredDatabase?: string | null) {
    const selected = datasources.find((item) => item.id === id) ?? null;
    setActiveId(id);
    setActiveDbType(selected?.db_type ?? null);
    setSelectedNodeId(null);
    setDetail(null);
    setSqlResult(null);
    setObjects([]);
    setObjectParentStack([]);
    setObjectPath([]);

    try {
      setLoadingDatabases(true);
      const databaseResponse = await apiClient.listDatabases(id, {
        page: 1,
        pageSize: DATABASE_PAGE_SIZE
      });
      setDatabases(databaseResponse.items);

      const preferred =
        preferredDatabase && databaseResponse.items.some((item) => item.name === preferredDatabase)
          ? preferredDatabase
          : databaseResponse.items[0]?.name ?? null;
      setSelectedDatabase(preferred);

      if (preferred) {
        await loadObjectsForParent(id, `db:${preferred}`, [preferred]);
      } else {
        setObjects([]);
        setObjectParentStack([]);
        setObjectPath([]);
      }
    } catch (caught) {
      setDatabases([]);
      setSelectedDatabase(null);
      setObjects([]);
      setObjectParentStack([]);
      setObjectPath([]);
      setError(caught instanceof Error ? caught.message : "Load databases failed");
    } finally {
      setLoadingDatabases(false);
    }
  }

  async function loadObjectsForParent(
    datasourceId: string,
    parentId: string,
    pathLabels: string[],
    nextStack?: string[]
  ) {
    try {
      setLoadingObjects(true);
      const response = await apiClient.getNodes(datasourceId, {
        parentId,
        page: 1,
        pageSize: NODE_PAGE_SIZE
      });
      setObjects(response.items);
      setObjectPath(pathLabels);
      setObjectParentStack(nextStack ?? [parentId]);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Load objects failed");
    } finally {
      setLoadingObjects(false);
    }
  }

  async function handleSelectDatasource(id: string) {
    await selectDatasource(id, null);
  }

  async function handleSelectDatabase(database: string) {
    if (!activeId) {
      return;
    }
    setSelectedDatabase(database);
    setSelectedNodeId(null);
    setDetail(null);
    await loadObjectsForParent(activeId, `db:${database}`, [database], [`db:${database}`]);
  }

  async function loadDetailIfSupported(node: MetadataNode) {
    if (!activeId) {
      return;
    }
    if (!supportsDetailByNodeIdForDb(node.id, activeDbType)) {
      setDetail(null);
      return;
    }

    try {
      const response = await apiClient.getObjectDetail(activeId, node.id);
      setDetail(response);
    } catch (caught) {
      setDetail(null);
      setError(caught instanceof Error ? caught.message : "Load object detail failed");
    }
  }

  async function handleOpenObject(node: MetadataNode) {
    if (!activeId) {
      return;
    }

    setInspectorTab("detail");
    setSelectedNodeId(node.id);
    await loadDetailIfSupported(node);

    if (!node.has_children) {
      return;
    }

    const nextStack = [...objectParentStack, node.id];
    const nextPath = [...objectPath, node.display_name];
    await loadObjectsForParent(activeId, node.id, nextPath, nextStack);
  }

  async function handleBackObject() {
    if (!activeId || objectParentStack.length <= 1) {
      return;
    }

    const nextStack = objectParentStack.slice(0, -1);
    const nextParent = nextStack[nextStack.length - 1];
    const nextPath = objectPath.slice(0, -1);
    setSelectedNodeId(nextParent);
    await loadObjectsForParent(activeId, nextParent, nextPath, nextStack);
  }

  async function handleCreateClick() {
    setModalMode("create");
    setEditingId(null);
    setEditingValue(null);
    setModalOpen(true);
  }

  async function handleEditClick(id: string) {
    try {
      setError(null);
      const detailResponse = await apiClient.getDatasource(id);
      setModalMode("edit");
      setEditingId(id);
      setEditingValue(detailToEditorValue(detailResponse));
      setModalOpen(true);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Load connection detail failed");
    }
  }

  async function handleDeleteConnection(id: string) {
    try {
      setError(null);
      await apiClient.deleteDatasource(id);
      const updatedItems = await refreshDatasourceList();

      if (activeId === id) {
        const next = updatedItems[0] ?? null;
        if (next) {
          await selectDatasource(next.id);
        } else {
          clearWorkspace();
        }
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Delete connection failed");
      throw caught;
    }
  }

  async function handleTestConnection(id: string) {
    try {
      setError(null);
      await apiClient.testDatasource(id);
      await refreshDatasourceList();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Test connection failed");
      throw caught;
    }
  }

  async function handleModalSubmit(value: ConnectionEditorValue) {
    setModalSubmitting(true);
    try {
      setError(null);
      if (modalMode === "create") {
        const payload = toCreatePayload(value);
        const response = await apiClient.createDatasource(payload);
        await refreshDatasourceList();
        setModalOpen(false);
        await selectDatasource(response.id);
      } else if (editingId) {
        const payload = toUpdatePayload(value);
        await apiClient.updateDatasource(editingId, payload);
        await refreshDatasourceList();
        setModalOpen(false);
        if (activeId === editingId) {
          const preferredDb =
            value.dbType === "mysql" && value.mysqlScope === "single" ? value.database : selectedDatabase;
          await selectDatasource(editingId, preferredDb);
        }
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Save connection failed");
      throw caught;
    } finally {
      setModalSubmitting(false);
    }
  }

  async function handleExecuteSql() {
    if (!activeId) {
      return;
    }

    try {
      setInspectorTab("sql");
      setError(null);
      setSqlLoading(true);
      const response = await apiClient.executeQuery({
        datasource_id: activeId,
        database: selectedDatabase,
        sql,
        max_rows: 500
      });
      setSqlResult(response);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Execute SQL failed");
    } finally {
      setSqlLoading(false);
    }
  }

  return (
    <AppShell>
      {error ? <div className="error">{error}</div> : null}

      <div className="workbench-layout">
        <ConnectionSidebar
          items={datasources}
          activeId={activeId}
          onSelect={handleSelectDatasource}
          onCreate={() => void handleCreateClick()}
          onEdit={(id) => void handleEditClick(id)}
          onDelete={handleDeleteConnection}
          onTest={handleTestConnection}
        />

        <SchemaBrowserPanel
          datasourceName={activeConnection?.name ?? null}
          loadingDatabases={loadingDatabases}
          databases={databases}
          selectedDatabase={selectedDatabase}
          onSelectDatabase={(database) => void handleSelectDatabase(database)}
          loadingObjects={loadingObjects}
          objectPath={objectPath}
          objects={objects}
          selectedNodeId={selectedNodeId}
          onBackObject={() => void handleBackObject()}
          onOpenObject={(node) => void handleOpenObject(node)}
        />

        <section className="workbench-column inspector-column">
          <InspectorTabs
            activeTab={inspectorTab}
            onChangeTab={setInspectorTab}
            detail={detail}
            disabled={!activeId}
            selectedDatabase={selectedDatabase}
            sql={sql}
            loading={sqlLoading}
            result={sqlResult}
            onSqlChange={setSql}
            onExecute={handleExecuteSql}
          />
        </section>
      </div>

      <ConnectionModal
        open={modalOpen}
        mode={modalMode}
        dbTypes={dbTypes}
        initialValue={editingValue}
        submitting={modalSubmitting}
        onClose={() => setModalOpen(false)}
        onSubmit={handleModalSubmit}
      />
    </AppShell>
  );
}
