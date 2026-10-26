#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> (tempfile::TempDir, AttachmentManager) {
        let directory = tempfile::tempdir().unwrap();
        let manager = AttachmentManager::new(directory.path().join("vault")).unwrap();
        (directory, manager)
    }

    fn scope() -> AttachmentScope {
        AttachmentScope {
            owner_user_id: "owner-1".into(),
            conversation_id: "conversation-1".into(),
            draft_generation: 1,
        }
    }

    fn candidate(bytes: Vec<u8>) -> AttachmentCandidate {
        AttachmentCandidate {
            name: "notes.txt".into(),
            media_type: "text/plain".into(),
            kind: AttachmentKind::File,
            origin: AttachmentOrigin::File,
            bytes,
        }
    }

    #[tokio::test]
    async fn stages_opaque_owner_scoped_attachment_and_resolves_exact_spec() {
        let (_directory, manager) = manager();
        let result = manager
            .stage_candidate_for_scope(scope(), candidate(b"hello".to_vec()))
            .await;
        assert!(result.errors.is_empty());
        let draft = &result.attachments[0];
        assert!(!draft.id.contains("notes.txt"));
        assert!(draft.preview_url.is_none());

        let specs = manager
            .resolve_specs(scope(), std::slice::from_ref(&draft.id))
            .unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].display_name, "notes.txt");
        assert_eq!(specs[0].byte_size, 5);
        assert_eq!(specs[0].sha256, digest(b"hello"));
        assert!(specs[0].authorized_local_ref.starts_with(REFERENCE_PREFIX));
    }

    #[tokio::test]
    async fn rejects_cross_owner_resolution_and_tampering() {
        let (_directory, manager) = manager();
        let result = manager
            .stage_candidate_for_scope(scope(), candidate(b"hello".to_vec()))
            .await;
        let draft = &result.attachments[0];
        let other = AttachmentScope {
            owner_user_id: "owner-2".into(),
            conversation_id: "conversation-1".into(),
            draft_generation: 1,
        };
        assert!(manager
            .resolve_specs(other, std::slice::from_ref(&draft.id))
            .is_err());

        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), draft.id.clone()))
            .cloned()
            .unwrap();
        fs::write(
            manager.scope_directory(&scope().key()).join(metadata.token),
            b"other",
        )
        .unwrap();
        assert!(manager
            .resolve_specs(scope(), std::slice::from_ref(&draft.id))
            .is_err());
    }

    #[tokio::test]
    async fn enforces_count_and_aggregate_byte_limits() {
        let (_directory, manager) = manager();
        let first = manager
            .stage_candidate_for_scope(scope(), candidate(vec![0; MAXIMUM_ATTACHMENT_BYTES - 1]))
            .await;
        assert_eq!(first.attachments.len(), 1);
        let second = manager
            .stage_candidate_for_scope(scope(), candidate(vec![0; 2]))
            .await;
        assert!(second.attachments.is_empty());
        assert_eq!(second.errors, ["附件总大小不能超过 20 MB"]);
    }

    #[tokio::test]
    async fn discard_removes_uncommitted_files_but_commit_preserves_them() {
        let (_directory, manager) = manager();
        let discarded = manager
            .stage_candidate_for_scope(scope(), candidate(b"discard".to_vec()))
            .await
            .attachments
            .remove(0);
        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), discarded.id.clone()))
            .cloned()
            .unwrap();
        let file = manager
            .scope_directory(&scope().key())
            .join(&metadata.token);
        manager
            .discard(AttachmentIDsInput {
                scope: scope(),
                attachment_ids: vec![discarded.id],
            })
            .unwrap();
        assert!(!file.exists());

        let committed = manager
            .stage_candidate_for_scope(scope(), candidate(b"commit".to_vec()))
            .await
            .attachments
            .remove(0);
        let metadata = manager
            .state()
            .unwrap()
            .drafts
            .get(&(scope().key(), committed.id.clone()))
            .cloned()
            .unwrap();
        let file = manager
            .scope_directory(&scope().key())
            .join(&metadata.token);
        manager.commit(scope(), &[committed.id]).unwrap();
        assert!(file.exists());
    }

    #[test]
    fn reauthorizes_verified_team_payload_for_the_exact_host_conversation() {
        let (_directory, manager) = manager();
        let target = AttachmentScope {
            owner_user_id: "owner-1".into(),
            conversation_id: "agent-team-room-agent-1".into(),
            draft_generation: 0,
        };
        let data = b"verified image bytes".to_vec();
        let payload = VerifiedAttachmentPayload {
            display_name: "design.png".into(),
            media_type: "image/png".into(),
            kind: "image".into(),
            origin: "file".into(),
            sha256: digest(&data),
            data,
        };
        let (ids, specs) = manager
            .authorize_verified_payloads(target.clone(), &[payload])
            .unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].display_name, "design.png");
        assert_eq!(specs[0].media_type, "image/png");
        assert_eq!(specs[0].byte_size, 20);
        manager.commit(target, &ids).unwrap();
    }

    #[tokio::test]
    async fn rejects_duplicate_team_draft_ids_before_persistence() {
        let (_directory, manager) = manager();
        let draft = manager
            .stage_candidate_for_scope(scope(), candidate(b"one copy".to_vec()))
            .await
            .attachments
            .remove(0);
        let error = manager
            .verified_draft_payloads(scope(), &[draft.id.clone(), draft.id])
            .unwrap_err();
        assert_eq!(error, "团队消息附件不能重复。");
    }

    #[test]
    fn stale_cleanup_is_bounded_to_owned_attachment_temporary_names() {
        let (_directory, manager) = manager();
        let scope_directory = manager.inner.root.join("owner").join("conversation");
        fs::create_dir_all(&scope_directory).unwrap();
        let token = "00000000-0000-0000-0000-000000000000";
        let payload_temporary = scope_directory.join(format!("{token}.tmp"));
        let metadata_temporary = scope_directory.join(format!("{token}.json.tmp"));
        let final_payload = scope_directory.join(token);
        let unrelated = scope_directory.join("notes.tmp");
        for path in [
            &payload_temporary,
            &metadata_temporary,
            &final_payload,
            &unrelated,
        ] {
            fs::write(path, b"value").unwrap();
        }
        let too_deep = scope_directory.join("nested").join(format!("{token}.tmp"));
        fs::create_dir_all(too_deep.parent().unwrap()).unwrap();
        fs::write(&too_deep, b"value").unwrap();

        let removed = cleanup_attachment_temporaries(
            &manager.inner.root,
            SystemTime::now() + Duration::from_secs(1),
        );
        assert_eq!(removed, 2);
        assert!(!payload_temporary.exists());
        assert!(!metadata_temporary.exists());
        assert!(final_payload.exists());
        assert!(unrelated.exists());
        assert!(too_deep.exists());
    }

    #[test]
    fn gives_each_agent_conversation_an_independent_authorized_reference() {
        let (_directory, manager) = manager();
        let data = b"verified image bytes".to_vec();
        let payload = VerifiedAttachmentPayload {
            display_name: "design.png".into(),
            media_type: "image/png".into(),
            kind: "image".into(),
            origin: "file".into(),
            sha256: digest(&data),
            data,
        };
        let first_scope = AttachmentScope::new("owner-1", "agent-team-room-agent-1");
        let second_scope = AttachmentScope::new("owner-1", "agent-team-room-agent-2");
        let (first_ids, first_specs) = manager
            .authorize_verified_payloads(first_scope.clone(), std::slice::from_ref(&payload))
            .unwrap();
        let (second_ids, second_specs) = manager
            .authorize_verified_payloads(second_scope.clone(), &[payload])
            .unwrap();
        assert_ne!(first_ids, second_ids);
        assert_ne!(
            first_specs[0].authorized_local_ref,
            second_specs[0].authorized_local_ref
        );
        manager.discard_ids(first_scope, first_ids);
        manager.discard_ids(second_scope, second_ids);
    }
}
