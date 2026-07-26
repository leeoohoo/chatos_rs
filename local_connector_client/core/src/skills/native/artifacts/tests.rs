// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use base64::Engine;
use lopdf::{dictionary, Document, Object, Stream};
use serde_json::{json, Value};
use uuid::Uuid;

use super::*;
use crate::WorkspaceState;

fn test_context() -> (PathBuf, LocalState, RelayRequest) {
    let root = std::env::temp_dir().join(format!("chatos-artifact-test-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    };
    let request = RelayRequest {
        _message_type: "skill_execute_request".to_string(),
        request_id: "request-1".to_string(),
        owner_user_id: Some("owner-1".to_string()),
        device_id: Some("device-1".to_string()),
        workspace_id: "workspace-1".to_string(),
        method: Some("POST".to_string()),
        path: Some("/skills/execute".to_string()),
        headers: BTreeMap::new(),
        body: Value::Null,
    };
    (root, state, request)
}

fn write_blank_pdf(path: &Path, page_count: usize) {
    let parent = path.parent().expect("PDF parent");
    fs::create_dir_all(parent).expect("PDF directory");
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let mut kids = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
        });
        kids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_count as u32,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).expect("save test PDF");
}

fn write_acroform_pdf(path: &Path) {
    let parent = path.parent().expect("PDF parent");
    fs::create_dir_all(parent).expect("PDF directory");
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();

    let text_field_id = document.new_object_id();
    let text_appearance_id =
        document.add_object(Stream::new(dictionary! {}, b"q 0 0 100 20 re S Q".to_vec()));
    let text_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => text_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 760.into(), 240.into(), 790.into()],
        "AP" => dictionary! { "N" => text_appearance_id },
    });
    document.objects.insert(
        text_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Tx",
            "T" => lopdf::text_string("profile.name"),
            "V" => lopdf::text_string("Alice"),
            "MaxLen" => 40,
            "Kids" => vec![Object::Reference(text_widget_id)],
        }),
    );

    let checkbox_field_id = document.new_object_id();
    let checkbox_off_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let checkbox_yes_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let checkbox_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => checkbox_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 710.into(), 56.into(), 730.into()],
        "AS" => "Off",
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => checkbox_off_id,
                "Yes" => checkbox_yes_id,
            }
        },
    });
    document.objects.insert(
        checkbox_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Btn",
            "T" => lopdf::text_string("terms.accepted"),
            "V" => "Off",
            "Kids" => vec![Object::Reference(checkbox_widget_id)],
        }),
    );

    let radio_field_id = document.new_object_id();
    let radio_basic_off_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let radio_basic_on_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let radio_basic_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => radio_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 660.into(), 56.into(), 680.into()],
        "AS" => "Basic",
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => radio_basic_off_id,
                "Basic" => radio_basic_on_id,
            }
        },
    });
    let radio_premium_off_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let radio_premium_on_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let radio_premium_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => radio_field_id,
        "P" => page_id,
        "Rect" => vec![76.into(), 660.into(), 96.into(), 680.into()],
        "AS" => "Off",
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => radio_premium_off_id,
                "Premium" => radio_premium_on_id,
            }
        },
    });
    document.objects.insert(
        radio_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Btn",
            "T" => lopdf::text_string("subscription.plan"),
            "Ff" => 1_i64 << 15,
            "V" => "Basic",
            "Kids" => vec![
                Object::Reference(radio_basic_widget_id),
                Object::Reference(radio_premium_widget_id),
            ],
        }),
    );

    let choice_field_id = document.new_object_id();
    let choice_appearance_id =
        document.add_object(Stream::new(dictionary! {}, b"q 0 0 100 20 re S Q".to_vec()));
    let choice_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => choice_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 610.into(), 240.into(), 640.into()],
        "AP" => dictionary! { "N" => choice_appearance_id },
    });
    document.objects.insert(
        choice_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Ch",
            "T" => lopdf::text_string("profile.region"),
            "Ff" => 1_i64 << 17,
            "V" => lopdf::text_string("cn"),
            "I" => vec![Object::Integer(0)],
            "Opt" => vec![
                Object::Array(vec![
                    lopdf::text_string("cn"),
                    lopdf::text_string("中国"),
                ]),
                Object::Array(vec![
                    lopdf::text_string("us"),
                    lopdf::text_string("United States"),
                ]),
            ],
            "Kids" => vec![Object::Reference(choice_widget_id)],
        }),
    );

    let editable_choice_field_id = document.new_object_id();
    let editable_choice_appearance_id =
        document.add_object(Stream::new(dictionary! {}, b"q 0 0 100 20 re S Q".to_vec()));
    let editable_choice_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => editable_choice_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 560.into(), 240.into(), 590.into()],
        "AP" => dictionary! { "N" => editable_choice_appearance_id },
    });
    document.objects.insert(
        editable_choice_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Ch",
            "T" => lopdf::text_string("profile.city"),
            "Ff" => (1_i64 << 17) | (1_i64 << 18),
            "V" => lopdf::text_string("上海"),
            "Opt" => vec![
                Object::Array(vec![
                    lopdf::text_string("beijing"),
                    lopdf::text_string("北京"),
                ]),
                Object::Array(vec![
                    lopdf::text_string("guangzhou"),
                    lopdf::text_string("广州"),
                ]),
            ],
            "Kids" => vec![Object::Reference(editable_choice_widget_id)],
        }),
    );

    let multi_choice_field_id = document.new_object_id();
    let multi_choice_appearance_id =
        document.add_object(Stream::new(dictionary! {}, b"q 0 0 100 20 re S Q".to_vec()));
    let multi_choice_widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => multi_choice_field_id,
        "P" => page_id,
        "Rect" => vec![36.into(), 490.into(), 240.into(), 540.into()],
        "AP" => dictionary! { "N" => multi_choice_appearance_id },
    });
    document.objects.insert(
        multi_choice_field_id,
        Object::Dictionary(dictionary! {
            "FT" => "Ch",
            "T" => lopdf::text_string("preferences.colors"),
            "Ff" => 1_i64 << 21,
            "V" => Object::Array(vec![
                lopdf::text_string("red"),
                lopdf::text_string("blue"),
            ]),
            "I" => vec![Object::Integer(0), Object::Integer(2)],
            "Opt" => vec![
                Object::Array(vec![
                    lopdf::text_string("red"),
                    lopdf::text_string("红色"),
                ]),
                Object::Array(vec![
                    lopdf::text_string("green"),
                    lopdf::text_string("绿色"),
                ]),
                Object::Array(vec![
                    lopdf::text_string("blue"),
                    lopdf::text_string("蓝色"),
                ]),
            ],
            "Kids" => vec![Object::Reference(multi_choice_widget_id)],
        }),
    );

    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Annots" => vec![
                Object::Reference(text_widget_id),
                Object::Reference(checkbox_widget_id),
                Object::Reference(radio_basic_widget_id),
                Object::Reference(radio_premium_widget_id),
                Object::Reference(choice_widget_id),
                Object::Reference(editable_choice_widget_id),
                Object::Reference(multi_choice_widget_id),
            ],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let acroform_id = document.add_object(dictionary! {
        "Fields" => vec![
            Object::Reference(text_field_id),
            Object::Reference(checkbox_field_id),
            Object::Reference(radio_field_id),
            Object::Reference(choice_field_id),
            Object::Reference(editable_choice_field_id),
            Object::Reference(multi_choice_field_id),
        ],
        "NeedAppearances" => false,
    });
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).expect("save AcroForm test PDF");
}

fn rewrite_zip_text_entry<F>(path: &Path, entry_name: &str, rewrite: F)
where
    F: FnOnce(String) -> String,
{
    let parent = path.parent().expect("ZIP parent");
    let mut archive = ZipArchive::new(File::open(path).expect("source ZIP")).expect("open ZIP");
    let temporary = NamedTempFile::new_in(parent).expect("temporary ZIP");
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut rewrite = Some(rewrite);
    let mut found = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("ZIP entry");
        let name = entry.name().to_string();
        if entry.is_dir() {
            writer.add_directory(name, options).expect("ZIP directory");
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).expect("read ZIP entry");
        if name == entry_name {
            let source = String::from_utf8(content).expect("UTF-8 ZIP text entry");
            content = rewrite.take().expect("single ZIP rewrite")(source).into_bytes();
            found = true;
        }
        writer.start_file(name, options).expect("start ZIP entry");
        writer
            .write_all(content.as_slice())
            .expect("write ZIP entry");
    }
    assert!(found, "missing ZIP entry {entry_name}");
    let temporary = writer.finish().expect("finish rewritten ZIP");
    drop(archive);
    fs::remove_file(path).expect("remove original ZIP");
    temporary.persist(path).expect("persist rewritten ZIP");
}

fn add_zip_entries(path: &Path, additions: &[(&str, &[u8])]) {
    let parent = path.parent().expect("ZIP parent");
    let mut archive = ZipArchive::new(File::open(path).expect("source ZIP")).expect("open ZIP");
    let temporary = NamedTempFile::new_in(parent).expect("temporary ZIP");
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let existing = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .expect("ZIP entry name")
                .name()
                .to_string()
        })
        .collect::<std::collections::HashSet<_>>();
    for (name, _) in additions {
        assert!(!existing.contains(*name), "duplicate ZIP addition {name}");
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("ZIP entry");
        let name = entry.name().to_string();
        if entry.is_dir() {
            writer.add_directory(name, options).expect("ZIP directory");
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).expect("read ZIP entry");
        writer.start_file(name, options).expect("start ZIP entry");
        writer
            .write_all(content.as_slice())
            .expect("write ZIP entry");
    }
    for (name, bytes) in additions {
        writer
            .start_file(*name, options)
            .expect("start added ZIP entry");
        writer.write_all(bytes).expect("write added ZIP entry");
    }
    let temporary = writer.finish().expect("finish ZIP additions");
    drop(archive);
    fs::remove_file(path).expect("remove original ZIP");
    temporary.persist(path).expect("persist ZIP additions");
}

fn add_standard_pptx_chart_fixture(path: &Path, slide_number: usize) {
    let slide_part = format!("ppt/slides/slide{slide_number}.xml");
    let relationships_part = format!("ppt/slides/_rels/slide{slide_number}.xml.rels");
    rewrite_zip_text_entry(path, slide_part.as_str(), |xml| {
        let chart_frame = concat!(
            "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"99\" name=\"Quarterly Sales Chart\"/>",
            "<p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>",
            "<p:xfrm><a:off x=\"609600\" y=\"1219200\"/><a:ext cx=\"10972800\" cy=\"4876800\"/></p:xfrm>",
            "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
            "<c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"rId2\"/>",
            "</a:graphicData></a:graphic></p:graphicFrame>"
        );
        xml.replacen(
            "</p:spTree>",
            format!("{chart_frame}</p:spTree>").as_str(),
            1,
        )
    });
    rewrite_zip_text_entry(path, relationships_part.as_str(), |xml| {
        xml.replacen(
            "</Relationships>",
            concat!(
                "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" ",
                "Target=\"../charts/chart1.xml\"/></Relationships>"
            ),
            1,
        )
    });
    rewrite_zip_text_entry(path, "[Content_Types].xml", |xml| {
        xml.replacen(
            "</Types>",
            concat!(
                "<Override PartName=\"/ppt/charts/chart1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>",
                "<Override PartName=\"/ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>",
                "</Types>"
            ),
            1,
        )
    });
    let chart_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
        "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
        "<c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r>",
        "<a:rPr lang=\"en-US\"/><a:t>Quarterly Sales</a:t></a:r><a:endParaRPr lang=\"en-US\"/>",
        "</a:p></c:rich></c:tx></c:title><c:plotArea><c:layout/><c:barChart>",
        "<c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>",
        "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef>",
        "<c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>North</c:v></c:pt></c:strCache>",
        "</c:strRef></c:tx><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$4</c:f><c:strCache>",
        "<c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>Q1</c:v></c:pt><c:pt idx=\"1\"><c:v>Q2</c:v></c:pt>",
        "<c:pt idx=\"2\"><c:v>Q3</c:v></c:pt></c:strCache></c:strRef></c:cat>",
        "<c:val><c:numRef><c:f>Sheet1!$B$2:$B$4</c:f><c:numCache><c:formatCode>General</c:formatCode>",
        "<c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>10</c:v></c:pt><c:pt idx=\"1\"><c:v>20</c:v></c:pt>",
        "<c:pt idx=\"2\"><c:v>30</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>",
        "<c:ser><c:idx val=\"1\"/><c:order val=\"1\"/><c:tx><c:strRef>",
        "<c:f>Sheet1!$C$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>South</c:v></c:pt></c:strCache>",
        "</c:strRef></c:tx><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$4</c:f><c:strCache>",
        "<c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>Q1</c:v></c:pt><c:pt idx=\"1\"><c:v>Q2</c:v></c:pt>",
        "<c:pt idx=\"2\"><c:v>Q3</c:v></c:pt></c:strCache></c:strRef></c:cat>",
        "<c:val><c:numRef><c:f>Sheet1!$C$2:$C$4</c:f><c:numCache><c:formatCode>General</c:formatCode>",
        "<c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt><c:pt idx=\"1\"><c:v>18</c:v></c:pt>",
        "<c:pt idx=\"2\"><c:v>36</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>",
        "<c:axId val=\"100\"/><c:axId val=\"200\"/></c:barChart></c:plotArea>",
        "<c:plotVisOnly val=\"1\"/></c:chart><c:externalData r:id=\"rId1\"><c:autoUpdate val=\"0\"/></c:externalData>",
        "</c:chartSpace>"
    );
    let chart_relationships = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/package\" ",
        "Target=\"../embeddings/Microsoft_Excel_Worksheet1.xlsx\"/></Relationships>"
    );
    add_zip_entries(
        path,
        &[
            ("ppt/charts/chart1.xml", chart_xml.as_bytes()),
            (
                "ppt/charts/_rels/chart1.xml.rels",
                chart_relationships.as_bytes(),
            ),
            (
                "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx",
                b"not-opened-by-chart-inspection",
            ),
        ],
    );
}

fn split_pptx_text_run(path: &Path, slide_part: &str, text: &str, chunks: &[&str]) {
    rewrite_zip_text_entry(path, slide_part, |xml| {
        assert_eq!(chunks.concat(), text);
        let text_element = format!("<a:t xml:space=\"preserve\">{text}</a:t>");
        let text_start = xml.find(text_element.as_str()).expect("PPTX text element");
        let run_start = xml[..text_start].rfind("<a:r>").expect("PPTX run start");
        let run_close = xml[text_start + text_element.len()..]
            .find("</a:r>")
            .map(|offset| text_start + text_element.len() + offset)
            .expect("PPTX run end");
        let run_end = run_close + "</a:r>".len();
        let properties = &xml[run_start + "<a:r>".len()..text_start];
        let split_runs = chunks
            .iter()
            .map(|chunk| {
                format!("<a:r>{properties}<a:t xml:space=\"preserve\">{chunk}</a:t></a:r>")
            })
            .collect::<String>();
        format!("{}{}{}", &xml[..run_start], split_runs, &xml[run_end..])
    });
}

fn insert_simple_pptx_table(path: &Path, slide_part: &str) {
    rewrite_zip_text_entry(path, slide_part, |xml| {
        let table = r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="42" name="Table 1"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="914400" y="1828800"/><a:ext cx="10363200" cy="2743200"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"><a:tableStyleId>{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}</a:tableStyleId></a:tblPr><a:tblGrid><a:gridCol w="5181600"/><a:gridCol w="5181600"/></a:tblGrid><a:tr h="1371600"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="1800" b="1"/><a:t>Region</a:t></a:r><a:endParaRPr lang="zh-CN" sz="1800"/></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="1800" b="1"/><a:t>Revenue</a:t></a:r><a:endParaRPr lang="zh-CN" sz="1800"/></a:p></a:txBody><a:tcPr/></a:tc></a:tr><a:tr h="1371600"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="1800"/><a:t>East</a:t></a:r><a:endParaRPr lang="zh-CN" sz="1800"/></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="1800"/><a:t>120</a:t></a:r><a:endParaRPr lang="zh-CN" sz="1800"/></a:p></a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#;
        let insertion = xml.rfind("</p:spTree>").expect("PPTX shape tree end");
        format!("{}{}{}", &xml[..insertion], table, &xml[insertion..])
    });
}

#[test]
fn creates_and_inspects_office_artifacts_locally() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({"target_path":"artifacts/demo.docx","title":"Demo","paragraphs":["First paragraph","Second paragraph"]}),
        &state,
        &request,
    )
    .expect("docx");
    let docx = inspect_docx(&json!({"path":"artifacts/demo.docx"}), &state, &request)
        .expect("inspect docx");
    assert!(docx
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("First paragraph")));
    assert_eq!(docx.pointer("/metadata/present"), Some(&Value::Bool(false)));
    let mut docx_archive =
        ZipArchive::new(File::open(root.join("artifacts/demo.docx")).expect("created DOCX"))
            .expect("created DOCX archive");
    let styles = read_zip_text(&mut docx_archive, "word/styles.xml").expect("DOCX styles");
    assert!(styles.contains("Noto Sans SC"));
    for style in [
        "Title", "Subtitle", "Heading1", "Heading2", "Heading3", "Quote",
    ] {
        assert!(
            styles.contains(format!(r#"w:styleId="{style}""#).as_str()),
            "missing {style} style"
        );
    }

    spreadsheet::create_xlsx(
        &json!({"target_path":"artifacts/demo.xlsx","sheet_name":"Data","rows":[["Name","Count"],["Apple",3]]}),
        &state,
        &request,
    )
    .expect("xlsx");
    let xlsx = inspect_spreadsheet(&json!({"path":"artifacts/demo.xlsx"}), &state, &request)
        .expect("inspect xlsx");
    assert_eq!(xlsx.get("worksheets").and_then(Value::as_u64), Some(1));

    presentation::create_pptx(
        &json!({"target_path":"artifacts/demo.pptx","slides":[{"title":"Demo","body":"Generated locally"}]}),
        &state,
        &request,
    )
    .expect("pptx");
    let pptx = presentation::inspect_pptx(&json!({"path":"artifacts/demo.pptx"}), &state, &request)
        .expect("inspect pptx");
    assert_eq!(pptx.get("slides").and_then(Value::as_u64), Some(1));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_inspects_and_updates_bounded_multi_sheet_xlsx() {
    let (root, state, request) = test_context();
    let created = spreadsheet::create_xlsx(
        &json!({
            "target_path":"artifacts/workbook.xlsx",
            "worksheets":[
                {
                    "name":"Data",
                    "freeze_rows":1,
                    "column_widths":[{"column":"A","width":18.0},{"column":"B","width":12.5}],
                    "rows":[
                        ["Item","Amount","Total"],
                        ["Apple",{"value":3.5,"number_format":"decimal_2"},{"formula":"SUM(B2:B3)","cached_value":7.0,"number_format":"decimal_2"}],
                        ["Pear",3.5,null]
                    ]
                },
                {
                    "name":"Summary",
                    "rows":[["Grand total"],[{"formula":"SUM(Data!B2:B3)","cached_value":7.0,"number_format":"decimal_2"}]]
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("multi-sheet XLSX");
    assert_eq!(created.get("worksheets").and_then(Value::as_u64), Some(2));
    assert_eq!(
        created.get("formula_cells").and_then(Value::as_u64),
        Some(2)
    );

    let inspected =
        inspect_spreadsheet(&json!({"path":"artifacts/workbook.xlsx"}), &state, &request)
            .expect("inspect multi-sheet XLSX");
    assert_eq!(inspected.get("worksheets").and_then(Value::as_u64), Some(2));
    assert_eq!(
        inspected.get("formula_cells").and_then(Value::as_u64),
        Some(2)
    );
    let sheets = inspected
        .get("sheets")
        .and_then(Value::as_array)
        .expect("sheet metadata");
    assert_eq!(
        sheets[0].get("frozen_rows").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        sheets[0]
            .get("custom_column_widths")
            .and_then(Value::as_u64),
        Some(2)
    );

    let source = root.join("artifacts/workbook.xlsx");
    let source_hash = sha256_file(source.as_path()).expect("source hash");
    spreadsheet::update_xlsx_range(
        &json!({
            "path":"artifacts/workbook.xlsx",
            "target_path":"artifacts/workbook-updated.xlsx",
            "sheet_name":"Data",
            "start_cell":"B2",
            "values":[[
                {"value":10.25,"number_format":"decimal_2"},
                {"formula":"SUM(B2:B3)","cached_value":13.75,"number_format":"decimal_2"}
            ]]
        }),
        &state,
        &request,
    )
    .expect("update XLSX range");
    assert_eq!(
        sha256_file(source.as_path()).expect("source hash after update"),
        source_hash
    );

    let updated = inspect_spreadsheet(
        &json!({"path":"artifacts/workbook-updated.xlsx"}),
        &state,
        &request,
    )
    .expect("inspect updated XLSX");
    assert_eq!(updated.get("worksheets").and_then(Value::as_u64), Some(2));
    assert_eq!(
        updated.get("formula_cells").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        updated
            .get("recalculation_on_open")
            .and_then(Value::as_bool),
        Some(true)
    );

    let mut original =
        ZipArchive::new(File::open(source).expect("source XLSX")).expect("source ZIP");
    let mut rewritten = ZipArchive::new(
        File::open(root.join("artifacts/workbook-updated.xlsx")).expect("updated XLSX"),
    )
    .expect("updated ZIP");
    assert_eq!(
        read_zip_text(&mut original, "xl/worksheets/sheet2.xml").expect("original sheet2"),
        read_zip_text(&mut rewritten, "xl/worksheets/sheet2.xml").expect("updated sheet2")
    );
    let sheet1 = read_zip_text(&mut rewritten, "xl/worksheets/sheet1.xml").expect("sheet1");
    assert!(sheet1.contains("<c r=\"B2\""));
    assert!(sheet1.contains("<v>10.25</v>"));
    assert!(sheet1.contains("<f>SUM(B2:B3)</f><v>13.75</v>"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn xlsx_updates_fail_closed_for_in_place_and_unsafe_formula_requests() {
    let (root, state, request) = test_context();
    spreadsheet::create_xlsx(
        &json!({"target_path":"book.xlsx","rows":[["A"],[1]]}),
        &state,
        &request,
    )
    .expect("source XLSX");
    let in_place = spreadsheet::update_xlsx_range(
        &json!({
            "path":"book.xlsx",
            "target_path":"book.xlsx",
            "sheet_name":"Sheet1",
            "start_cell":"A1",
            "values":[[2]],
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place XLSX update must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let unsafe_formula = spreadsheet::create_xlsx(
        &json!({
            "target_path":"unsafe.xlsx",
            "rows":[[{"formula":"WEBSERVICE(A1)"}]]
        }),
        &state,
        &request,
    )
    .expect_err("unsafe formula must fail");
    assert!(unsafe_formula.to_string().contains("safety allowlist"));

    create_csv(
        &json!({
            "target_path":"safe.csv",
            "rows":[["=SUM(A1:A2)"," -text",-3,"ordinary"]]
        }),
        &state,
        &request,
    )
    .expect("safe CSV");
    let csv = fs::read_to_string(root.join("safe.csv")).expect("read CSV");
    assert_eq!(csv, "'=SUM(A1:A2),' -text,-3,ordinary\r\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_inspects_and_safely_updates_bounded_tsv() {
    let (root, state, request) = test_context();
    let created = create_tsv(
        &json!({
            "target_path":"artifacts/source.tsv",
            "rows":[
                ["Name","Note","Value"],
                ["Alice","tab\tline\n\"quote\"","=SUM(A1:A2)"],
                ["Bob",null,-3]
            ]
        }),
        &state,
        &request,
    )
    .expect("create TSV");
    assert_eq!(created.get("rows").and_then(Value::as_u64), Some(3));
    assert_eq!(created.get("columns").and_then(Value::as_u64), Some(3));

    let source = root.join("artifacts/source.tsv");
    let source_text = fs::read_to_string(source.as_path()).expect("source TSV");
    assert_eq!(
        source_text,
        "Name\tNote\tValue\r\nAlice\t\"tab\tline\n\"\"quote\"\"\"\t'=SUM(A1:A2)\r\nBob\t\t-3\r\n"
    );
    let inspected = inspect_spreadsheet(&json!({"path":"artifacts/source.tsv"}), &state, &request)
        .expect("inspect TSV");
    assert_eq!(inspected.get("format").and_then(Value::as_str), Some("tsv"));
    assert_eq!(inspected.get("rows").and_then(Value::as_u64), Some(3));
    assert_eq!(inspected.get("columns").and_then(Value::as_u64), Some(3));
    assert_eq!(
        inspected.get("rectangular").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspected.get("line_ending").and_then(Value::as_str),
        Some("crlf")
    );
    let source_sha256 = inspected
        .get("sha256")
        .and_then(Value::as_str)
        .expect("TSV SHA-256")
        .to_string();

    let updated = update_tsv_range(
        &json!({
            "path":"artifacts/source.tsv",
            "expected_sha256":source_sha256,
            "start_cell":"B2",
            "end_cell":"C2",
            "values":[["changed\tvalue","+danger"]],
            "target_path":"artifacts/updated.tsv"
        }),
        &state,
        &request,
    )
    .expect("update TSV");
    assert_eq!(
        updated.get("updated_cells").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        sha256_file(source.as_path()).expect("source hash after update"),
        source_sha256
    );
    let updated_text = fs::read_to_string(root.join("artifacts/updated.tsv")).expect("updated TSV");
    let parsed = parse_tsv(updated_text.as_str()).expect("parse updated TSV");
    assert_eq!(parsed.rows[0], vec!["Name", "Note", "Value"]);
    assert_eq!(parsed.rows[1], vec!["Alice", "changed\tvalue", "'+danger"]);
    assert_eq!(parsed.rows[2], vec!["Bob", "", "-3"]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tsv_updates_fail_closed_for_stale_geometry_ragged_and_unsafe_paths() {
    let (root, state, request) = test_context();
    let empty_row = create_tsv(
        &json!({"target_path":"empty-row.tsv","rows":[[]]}),
        &state,
        &request,
    )
    .expect_err("zero-cell TSV row must fail");
    assert!(empty_row.to_string().contains("at least one cell"));
    create_tsv(
        &json!({"target_path":"source.tsv","rows":[["a","b"],["c","d"]]}),
        &state,
        &request,
    )
    .expect("source TSV");
    let source = root.join("source.tsv");
    let source_sha256 = sha256_file(source.as_path()).expect("source hash");

    let stale = update_tsv_range(
        &json!({
            "path":"source.tsv",
            "expected_sha256":"0".repeat(64),
            "start_cell":"A1",
            "end_cell":"A1",
            "values":[["x"]],
            "target_path":"stale.tsv"
        }),
        &state,
        &request,
    )
    .expect_err("stale TSV hash must fail");
    assert!(stale.to_string().contains("expected_sha256"));
    assert!(!root.join("stale.tsv").exists());

    let wrong_geometry = update_tsv_range(
        &json!({
            "path":"source.tsv",
            "expected_sha256":source_sha256,
            "start_cell":"A1",
            "end_cell":"B2",
            "values":[["x","y"]],
            "target_path":"wrong.tsv"
        }),
        &state,
        &request,
    )
    .expect_err("wrong TSV geometry must fail");
    assert!(wrong_geometry.to_string().contains("geometry"));
    assert!(!root.join("wrong.tsv").exists());

    let in_place = update_tsv_range(
        &json!({
            "path":"source.tsv",
            "expected_sha256":source_sha256,
            "start_cell":"A1",
            "end_cell":"A1",
            "values":[["x"]],
            "target_path":"source.tsv",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place TSV update must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    fs::hard_link(source.as_path(), root.join("source-hard-link.tsv"))
        .expect("TSV source hard link");
    let hard_link = update_tsv_range(
        &json!({
            "path":"source.tsv",
            "expected_sha256":source_sha256,
            "start_cell":"A1",
            "end_cell":"A1",
            "values":[["x"]],
            "target_path":"source-hard-link.tsv",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("hard-linked TSV target must fail");
    assert!(hard_link.to_string().contains("distinct target_path"));

    fs::write(root.join("ragged.tsv"), b"a\tb\r\nc\r\n").expect("ragged TSV");
    let ragged_inspection = inspect_spreadsheet(&json!({"path":"ragged.tsv"}), &state, &request)
        .expect("inspect ragged TSV");
    assert_eq!(
        ragged_inspection
            .get("rectangular")
            .and_then(Value::as_bool),
        Some(false)
    );
    let ragged_hash = ragged_inspection
        .get("sha256")
        .and_then(Value::as_str)
        .expect("ragged TSV hash");
    let ragged = update_tsv_range(
        &json!({
            "path":"ragged.tsv",
            "expected_sha256":ragged_hash,
            "start_cell":"A1",
            "end_cell":"A1",
            "values":[["x"]],
            "target_path":"ragged-updated.tsv"
        }),
        &state,
        &request,
    )
    .expect_err("ragged TSV update must fail");
    assert!(ragged.to_string().contains("rectangular"));

    let oversize_path = root.join("oversize.tsv");
    File::create(oversize_path.as_path())
        .expect("oversize TSV")
        .set_len(MAX_ARTIFACT_BYTES + 1)
        .expect("oversize TSV length");
    let oversize = inspect_spreadsheet(&json!({"path":"oversize.tsv"}), &state, &request)
        .expect_err("oversize TSV must fail");
    assert!(oversize.to_string().contains("100 MiB"));

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source.as_path(), root.join("source-link.tsv"))
            .expect("TSV source symlink");
        let symlink = inspect_spreadsheet(&json!({"path":"source-link.tsv"}), &state, &request)
            .expect_err("TSV source symlink must fail");
        assert!(symlink.to_string().contains("non-symlink"));

        std::os::unix::fs::symlink(root.join("target-real.tsv"), root.join("target-link.tsv"))
            .expect("TSV target symlink");
        fs::write(root.join("target-real.tsv"), b"old\r\n").expect("TSV target");
        let target_symlink = update_tsv_range(
            &json!({
                "path":"source.tsv",
                "expected_sha256":source_sha256,
                "start_cell":"A1",
                "end_cell":"A1",
                "values":[["x"]],
                "target_path":"target-link.tsv",
                "overwrite":true
            }),
            &state,
            &request,
        )
        .expect_err("TSV target symlink must fail");
        assert!(target_symlink.to_string().contains("non-symlink"));
    }

    assert_eq!(
        sha256_file(source.as_path()).expect("source hash after failures"),
        source_sha256
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tsv_parser_rejects_ambiguous_quoting_and_mixed_record_endings() {
    let quote = parse_tsv("a\tbad\"quote\r\n").expect_err("unquoted TSV quote must fail");
    assert!(quote.to_string().contains("quote must begin"));
    let trailing = parse_tsv("a\t\"quoted\"tail\r\n")
        .expect_err("characters after a quoted TSV field must fail");
    assert!(trailing.to_string().contains("closing quote"));
    let mixed = parse_tsv("a\tb\r\nc\td\n").expect_err("mixed TSV endings must fail");
    assert!(mixed.to_string().contains("mixed"));
}

#[test]
fn creates_and_inspects_pptx_layouts_images_and_speaker_notes() {
    let (root, state, request) = test_context();
    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::create_dir_all(root.join("assets")).expect("assets");
    fs::write(root.join("assets/pixel.png"), image).expect("write PNG");
    let image_before = fs::read(root.join("assets/pixel.png")).expect("image bytes");

    let created = presentation::create_pptx(
        &json!({
            "target_path":"artifacts/deck.pptx",
            "slides":[
                {
                    "title":"Quarterly Review",
                    "layout":"title_body",
                    "body":"- Revenue grew\n- Retention improved",
                    "notes":"Open with the customer outcome, then explain the metrics."
                },
                {
                    "title":"Comparison",
                    "layout":"two_column",
                    "left_body":"Current\n- Manual review",
                    "right_body":"Target\n- Automated checks"
                },
                {
                    "title":"Architecture",
                    "layout":"image_right",
                    "body":"The image remains editable and aspect-ratio constrained.",
                    "image":{"path":"assets/pixel.png","alt_text":"Architecture diagram","fit":"contain"}
                },
                {
                    "title":"Customer Experience",
                    "layout":"image_full",
                    "body":"A full-bleed image with readable overlays.",
                    "image":{"path":"assets/pixel.png","alt_text":"Customer experience visual","fit":"cover"}
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("create rich PPTX");
    assert_eq!(created.get("slides").and_then(Value::as_u64), Some(4));
    assert_eq!(created.get("images").and_then(Value::as_u64), Some(2));
    assert_eq!(
        created.get("speaker_notes").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        fs::read(root.join("assets/pixel.png")).expect("image after create"),
        image_before
    );

    let inspected =
        presentation::inspect_pptx(&json!({"path":"artifacts/deck.pptx"}), &state, &request)
            .expect("inspect rich PPTX");
    assert_eq!(inspected.get("slides").and_then(Value::as_u64), Some(4));
    assert_eq!(inspected.get("images").and_then(Value::as_u64), Some(2));
    assert_eq!(
        inspected.get("media_files").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        inspected.get("speaker_notes").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        inspected.get("widescreen").and_then(Value::as_bool),
        Some(true)
    );
    let slides = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        slides[0].get("title").and_then(Value::as_str),
        Some("Quarterly Review")
    );
    assert!(slides[0]
        .get("notes_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("customer outcome")));

    let mut archive =
        ZipArchive::new(File::open(root.join("artifacts/deck.pptx")).expect("deck file"))
            .expect("deck ZIP");
    assert!(archive.by_name("ppt/notesMasters/notesMaster1.xml").is_ok());
    assert!(archive.by_name("ppt/notesSlides/notesSlide1.xml").is_ok());
    assert!(archive.by_name("ppt/media/image1.png").is_ok());
    assert!(archive.by_name("ppt/media/image2.png").is_ok());
    let slide = read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("slide XML");
    assert!(slide.contains("<a:buChar char=\"•\"/>"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_inspects_and_edits_simple_pptx_table_layout() {
    let (root, state, request) = test_context();
    let created = presentation::create_pptx(
        &json!({
            "target_path":"table.pptx",
            "slides":[{
                "title":"Quarterly metrics",
                "layout":"table",
                "table":{
                    "cells":[
                        ["Region","Revenue"],
                        ["East & West","<120>"],
                        ["华北",""]
                    ]
                }
            }]
        }),
        &state,
        &request,
    )
    .expect("create table-layout PPTX");
    assert_eq!(
        created.pointer("/layouts/0").and_then(Value::as_str),
        Some("table")
    );

    let inspected = presentation::inspect_pptx(&json!({"path":"table.pptx"}), &state, &request)
        .expect("inspect table-layout PPTX");
    assert_eq!(inspected.get("tables").and_then(Value::as_u64), Some(1));
    assert_eq!(
        inspected
            .pointer("/slide_metadata/0/table_metadata/0/eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    let table = presentation::inspect_pptx_table(
        &json!({"path":"table.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect generated table");
    assert_eq!(table.get("rows").and_then(Value::as_u64), Some(3));
    assert_eq!(table.get("columns").and_then(Value::as_u64), Some(2));
    assert_eq!(
        table.pointer("/cell_text/1/0").and_then(Value::as_str),
        Some("East & West")
    );
    assert_eq!(
        table.pointer("/cell_text/1/1").and_then(Value::as_str),
        Some("<120>")
    );
    assert_eq!(
        table.pointer("/cell_text/2/1").and_then(Value::as_str),
        Some("")
    );

    presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"table.pptx",
            "target_path":"edited-table.pptx",
            "slide_number":1,
            "table_number":1,
            "row":2,
            "column":2,
            "expected_text":"<120>",
            "replacement":"145 & rising"
        }),
        &state,
        &request,
    )
    .expect("edit generated table cell");
    let edited = presentation::inspect_pptx_table(
        &json!({"path":"edited-table.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect edited generated table");
    assert_eq!(
        edited.pointer("/cell_text/1/1").and_then(Value::as_str),
        Some("145 & rising")
    );

    let mut archive =
        ZipArchive::new(File::open(root.join("table.pptx")).expect("table PPTX file"))
            .expect("table PPTX ZIP");
    let slide = read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("table slide XML");
    assert!(slide.contains(
        "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">"
    ));
    assert!(slide.contains("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"));
    assert!(slide.contains("East &amp; West"));
    assert!(slide.contains("&lt;120&gt;"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_and_deletes_simple_pptx_table_rows_by_visible_slide_order() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"fixture.pptx",
            "slides":[
                {
                    "title":"Regional revenue",
                    "layout":"table",
                    "table":{"cells":[["Region","Revenue"],["East","120"],["West","90"]]}
                },
                {"title":"Other","body":"Untouched"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create row-edit fixture");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"fixture.pptx",
            "target_path":"source.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder row-edit fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("row-edit source bytes");
    let source_table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect row-edit source");
    assert_eq!(
        source_table
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(true)
    );

    let inserted = presentation::insert_pptx_table_row(
        &json!({
            "path":"source.pptx",
            "target_path":"inserted.pptx",
            "slide_number":2,
            "table_number":1,
            "reference_row":2,
            "position":"after",
            "expected_cells":["East","120"],
            "cells":["Central & South","75"]
        }),
        &state,
        &request,
    )
    .expect("insert PPTX table row");
    assert_eq!(
        inserted.get("inserted_row").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(inserted.get("rows").and_then(Value::as_u64), Some(4));
    assert_eq!(
        inserted
            .get("table_frame_height_unchanged")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after row insertion"),
        source_before
    );
    let inserted_before = fs::read(root.join("inserted.pptx")).expect("inserted bytes");
    let inserted_table = presentation::inspect_pptx_table(
        &json!({"path":"inserted.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect inserted PPTX table row");
    assert_eq!(inserted_table.get("rows").and_then(Value::as_u64), Some(4));
    assert_eq!(
        inserted_table
            .pointer("/cell_text/2/0")
            .and_then(Value::as_str),
        Some("Central & South")
    );
    assert_eq!(
        inserted_table
            .get("eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inserted_table
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(true)
    );

    let deleted = presentation::delete_pptx_table_row(
        &json!({
            "path":"inserted.pptx",
            "target_path":"deleted-row.pptx",
            "slide_number":2,
            "table_number":1,
            "row":2,
            "expected_cells":["East","120"]
        }),
        &state,
        &request,
    )
    .expect("delete PPTX table row");
    assert_eq!(deleted.get("deleted_row").and_then(Value::as_u64), Some(2));
    assert_eq!(deleted.get("rows").and_then(Value::as_u64), Some(3));
    assert_eq!(
        deleted
            .get("height_transferred_to_row")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        fs::read(root.join("inserted.pptx")).expect("inserted source after deletion"),
        inserted_before
    );
    let deleted_table = presentation::inspect_pptx_table(
        &json!({"path":"deleted-row.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect deleted PPTX table row");
    assert_eq!(
        deleted_table
            .pointer("/cell_text/1/0")
            .and_then(Value::as_str),
        Some("Central & South")
    );
    assert_eq!(
        deleted_table
            .pointer("/cell_text/2/0")
            .and_then(Value::as_str),
        Some("West")
    );

    presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"deleted-row.pptx",
            "target_path":"edited-after-row-ops.pptx",
            "slide_number":2,"table_number":1,"row":2,"column":2,
            "expected_text":"75","replacement":"80"
        }),
        &state,
        &request,
    )
    .expect("edit cell after row operations");
    let final_table = presentation::inspect_pptx_table(
        &json!({"path":"edited-after-row-ops.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect cell after row operations");
    assert_eq!(
        final_table
            .pointer("/cell_text/1/1")
            .and_then(Value::as_str),
        Some("80")
    );
    let mut archive =
        ZipArchive::new(File::open(root.join("deleted-row.pptx")).expect("deleted-row PPTX file"))
            .expect("deleted-row PPTX ZIP");
    let slide = read_zip_text(&mut archive, "ppt/slides/slide1.xml")
        .expect("structurally edited table slide XML");
    assert!(slide.contains("<a:ext cx=\"10820400\" cy=\"4754880\"/>"));
    assert!(slide.contains("Central &amp; South"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_stale_unsafe_or_in_place_pptx_table_row_edits_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{
                "title":"Rows","layout":"table",
                "table":{"cells":[["Name","Value"],["Alpha","1"]]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create row rejection fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("row rejection source bytes");

    let stale = presentation::delete_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"stale-row.pptx",
            "slide_number":1,"table_number":1,"row":2,
            "expected_cells":["Alpha","2"]
        }),
        &state,
        &request,
    )
    .expect_err("stale row snapshot must fail");
    assert!(stale.to_string().contains("does not match expected_cells"));

    let wrong_count = presentation::insert_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"wrong-count.pptx",
            "slide_number":1,"table_number":1,"reference_row":2,"position":"after",
            "expected_cells":["Alpha","1"],"cells":["Beta"]
        }),
        &state,
        &request,
    )
    .expect_err("wrong inserted cell count must fail");
    assert!(wrong_count.to_string().contains("exactly 2 cell strings"));

    let in_place = presentation::insert_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"source.pptx",
            "slide_number":1,"table_number":1,"reference_row":2,"position":"before",
            "expected_cells":["Alpha","1"],"cells":["Beta","2"],"overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place row insertion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    rewrite_zip_text_entry(
        root.join("source.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:tr h=\"", "<a:tr custom=\"1\" h=\"", 1),
    );
    let attributed_before =
        fs::read(root.join("source.pptx")).expect("attributed row source bytes");
    let inspection = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect attributed table row");
    assert_eq!(
        inspection
            .get("eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspection
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(false)
    );
    let unsafe_row = presentation::delete_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"unsafe-row.pptx",
            "slide_number":1,"table_number":1,"row":2,
            "expected_cells":["Alpha","1"]
        }),
        &state,
        &request,
    )
    .expect_err("attributed table row must fail");
    assert!(unsafe_row.to_string().contains("canonical a:tr"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after unsafe row failure"),
        attributed_before
    );

    presentation::create_pptx(
        &json!({
            "target_path":"one-row.pptx",
            "slides":[{"title":"Only","layout":"table","table":{"cells":[["Only"]]}}]
        }),
        &state,
        &request,
    )
    .expect("create one-row table");
    let only = presentation::delete_pptx_table_row(
        &json!({
            "path":"one-row.pptx","target_path":"empty-table.pptx",
            "slide_number":1,"table_number":1,"row":1,"expected_cells":["Only"]
        }),
        &state,
        &request,
    )
    .expect_err("only PPTX table row must not be deleted");
    assert!(only.to_string().contains("only row"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after all row failures"),
        attributed_before
    );
    assert_ne!(
        fs::read(root.join("source.pptx")).expect("attributed source"),
        source_before
    );
    for path in [
        "stale-row.pptx",
        "wrong-count.pptx",
        "unsafe-row.pptx",
        "empty-table.pptx",
    ] {
        assert!(
            !root.join(path).exists(),
            "unexpected row-edit output: {path}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_and_deletes_simple_pptx_table_columns_by_visible_slide_order() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"fixture.pptx",
            "slides":[
                {
                    "title":"Regional revenue",
                    "layout":"table",
                    "table":{"cells":[
                        ["Region","Revenue","Growth"],
                        ["East","120","10%"],
                        ["West","90","5%"]
                    ]}
                },
                {"title":"Other","body":"Untouched"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create column-edit fixture");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"fixture.pptx",
            "target_path":"source.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder column-edit fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("column-edit source bytes");
    let source_table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect column-edit source");
    assert_eq!(
        source_table
            .get("eligible_for_column_editing")
            .and_then(Value::as_bool),
        Some(true)
    );

    let inserted = presentation::insert_pptx_table_column(
        &json!({
            "path":"source.pptx",
            "target_path":"inserted-column.pptx",
            "slide_number":2,
            "table_number":1,
            "reference_column":1,
            "position":"after",
            "expected_cells":["Region","East","West"],
            "cells":["Segment","Retail & SMB","Enterprise"]
        }),
        &state,
        &request,
    )
    .expect("insert PPTX table column");
    assert_eq!(
        inserted.get("inserted_column").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(inserted.get("columns").and_then(Value::as_u64), Some(4));
    assert_eq!(
        inserted
            .get("table_frame_width_unchanged")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after column insertion"),
        source_before
    );
    let inserted_before =
        fs::read(root.join("inserted-column.pptx")).expect("inserted column bytes");
    let inserted_table = presentation::inspect_pptx_table(
        &json!({"path":"inserted-column.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect inserted PPTX table column");
    assert_eq!(
        inserted_table.get("columns").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        inserted_table
            .pointer("/cell_text/1/1")
            .and_then(Value::as_str),
        Some("Retail & SMB")
    );
    assert_eq!(
        inserted_table
            .get("eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inserted_table
            .get("eligible_for_column_editing")
            .and_then(Value::as_bool),
        Some(true)
    );

    let deleted = presentation::delete_pptx_table_column(
        &json!({
            "path":"inserted-column.pptx",
            "target_path":"deleted-column.pptx",
            "slide_number":2,
            "table_number":1,
            "column":3,
            "expected_cells":["Revenue","120","90"]
        }),
        &state,
        &request,
    )
    .expect("delete PPTX table column");
    assert_eq!(
        deleted.get("deleted_column").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(deleted.get("columns").and_then(Value::as_u64), Some(3));
    assert_eq!(
        deleted
            .get("width_transferred_to_column")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        fs::read(root.join("inserted-column.pptx")).expect("inserted source after deletion"),
        inserted_before
    );
    let deleted_table = presentation::inspect_pptx_table(
        &json!({"path":"deleted-column.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect deleted PPTX table column");
    assert_eq!(
        deleted_table
            .pointer("/cell_text/0/1")
            .and_then(Value::as_str),
        Some("Segment")
    );
    assert_eq!(
        deleted_table
            .pointer("/cell_text/1/2")
            .and_then(Value::as_str),
        Some("10%")
    );

    presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"deleted-column.pptx",
            "target_path":"edited-after-column-ops.pptx",
            "slide_number":2,"table_number":1,"row":3,"column":3,
            "expected_text":"5%","replacement":"6%"
        }),
        &state,
        &request,
    )
    .expect("edit cell after column operations");
    let final_table = presentation::inspect_pptx_table(
        &json!({"path":"edited-after-column-ops.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect cell after column operations");
    assert_eq!(
        final_table
            .pointer("/cell_text/2/2")
            .and_then(Value::as_str),
        Some("6%")
    );
    let mut archive = ZipArchive::new(
        File::open(root.join("deleted-column.pptx")).expect("deleted-column PPTX file"),
    )
    .expect("deleted-column PPTX ZIP");
    let slide = read_zip_text(&mut archive, "ppt/slides/slide1.xml")
        .expect("column-edited table slide XML");
    assert!(slide.contains("<a:ext cx=\"10820400\" cy=\"4754880\"/>"));
    assert_eq!(slide.matches("<a:gridCol w=\"1803400\"/>").count(), 2);
    assert!(slide.contains("<a:gridCol w=\"7213600\"/>"));
    assert!(slide.contains("Retail &amp; SMB"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_stale_unsafe_or_in_place_pptx_table_column_edits_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{
                "title":"Columns","layout":"table",
                "table":{"cells":[["Name","Value"],["Alpha","1"]]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create column rejection fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("column rejection source bytes");

    let stale = presentation::delete_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"stale-column.pptx",
            "slide_number":1,"table_number":1,"column":2,
            "expected_cells":["Value","2"]
        }),
        &state,
        &request,
    )
    .expect_err("stale column snapshot must fail");
    assert!(stale.to_string().contains("does not match expected_cells"));

    let wrong_count = presentation::insert_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"wrong-column-count.pptx",
            "slide_number":1,"table_number":1,"reference_column":1,"position":"after",
            "expected_cells":["Name","Alpha"],"cells":["Category"]
        }),
        &state,
        &request,
    )
    .expect_err("wrong inserted column cell count must fail");
    assert!(wrong_count.to_string().contains("exactly 2 cell strings"));

    let in_place = presentation::insert_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"source.pptx",
            "slide_number":1,"table_number":1,"reference_column":1,"position":"before",
            "expected_cells":["Name","Alpha"],"cells":["Category","A"],"overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place column insertion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    rewrite_zip_text_entry(
        root.join("source.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:gridCol w=\"", "<a:gridCol custom=\"1\" w=\"", 1),
    );
    let attributed_before =
        fs::read(root.join("source.pptx")).expect("attributed column source bytes");
    let inspection = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect attributed grid column");
    assert_eq!(
        inspection
            .get("eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspection
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspection
            .get("eligible_for_column_editing")
            .and_then(Value::as_bool),
        Some(false)
    );
    let unsafe_column = presentation::delete_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"unsafe-column.pptx",
            "slide_number":1,"table_number":1,"column":2,
            "expected_cells":["Value","1"]
        }),
        &state,
        &request,
    )
    .expect_err("attributed grid column must fail");
    assert!(unsafe_column.to_string().contains("canonical a:gridCol"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after unsafe column failure"),
        attributed_before
    );

    presentation::create_pptx(
        &json!({
            "target_path":"one-column.pptx",
            "slides":[{
                "title":"Only","layout":"table","table":{"cells":[["Only"],["Value"]]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create one-column table");
    let only = presentation::delete_pptx_table_column(
        &json!({
            "path":"one-column.pptx","target_path":"empty-columns.pptx",
            "slide_number":1,"table_number":1,"column":1,
            "expected_cells":["Only","Value"]
        }),
        &state,
        &request,
    )
    .expect_err("only PPTX table column must not be deleted");
    assert!(only.to_string().contains("only column"));

    presentation::create_pptx(
        &json!({
            "target_path":"narrow-column.pptx",
            "slides":[{
                "title":"Narrow","layout":"table",
                "table":{"cells":[["Name","Value"],["Alpha","1"]]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create narrow-column fixture");
    rewrite_zip_text_entry(
        root.join("narrow-column.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:gridCol w=\"5410200\"/>", "<a:gridCol w=\"1\"/>", 1),
    );
    let too_narrow = presentation::insert_pptx_table_column(
        &json!({
            "path":"narrow-column.pptx","target_path":"split-narrow-column.pptx",
            "slide_number":1,"table_number":1,"reference_column":1,"position":"after",
            "expected_cells":["Name","Alpha"],"cells":["Category","A"]
        }),
        &state,
        &request,
    )
    .expect_err("one-unit grid column must not split");
    assert!(too_narrow.to_string().contains("too narrow"));

    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after all column failures"),
        attributed_before
    );
    assert_ne!(
        fs::read(root.join("source.pptx")).expect("attributed source"),
        source_before
    );
    for path in [
        "stale-column.pptx",
        "wrong-column-count.pptx",
        "unsafe-column.pptx",
        "empty-columns.pptx",
        "split-narrow-column.pptx",
    ] {
        assert!(
            !root.join(path).exists(),
            "unexpected column-edit output: {path}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn moves_simple_pptx_table_rows_and_columns_by_visible_slide_order() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"fixture.pptx",
            "slides":[
                {
                    "title":"Quarterly revenue",
                    "layout":"table",
                    "table":{"cells":[
                        ["Region","Q1","Q2","Q3"],
                        ["North","10","20","30"],
                        ["South","1","2","3"],
                        ["West","7","8","9"]
                    ]}
                },
                {"title":"Other","body":"Untouched"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create table-move fixture");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"fixture.pptx",
            "target_path":"source.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder table-move fixture");
    rewrite_zip_text_entry(
        root.join("source.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| {
            let xml = xml.replace(
                concat!(
                    "<a:tblGrid>",
                    "<a:gridCol w=\"2705100\"/>",
                    "<a:gridCol w=\"2705100\"/>",
                    "<a:gridCol w=\"2705100\"/>",
                    "<a:gridCol w=\"2705100\"/>",
                    "</a:tblGrid>"
                ),
                concat!(
                    "<a:tblGrid>",
                    "<a:gridCol w=\"1000000\"/>",
                    "<a:gridCol w=\"2000000\"/>",
                    "<a:gridCol w=\"3000000\"/>",
                    "<a:gridCol w=\"4820400\"/>",
                    "</a:tblGrid>"
                ),
            );
            let xml = xml.replacen("<a:tr h=\"1188720\">", "<a:tr h=\"1000000\">", 1);
            let xml = xml.replacen("<a:tr h=\"1188720\">", "<a:tr h=\"1100000\">", 1);
            let xml = xml.replacen("<a:tr h=\"1188720\">", "<a:tr h=\"1200000\">", 1);
            xml.replacen("<a:tr h=\"1188720\">", "<a:tr h=\"1454880\">", 1)
        },
    );
    let source_before = fs::read(root.join("source.pptx")).expect("table-move source bytes");
    let source_table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect table-move source");
    assert_eq!(
        source_table
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        source_table
            .get("eligible_for_column_editing")
            .and_then(Value::as_bool),
        Some(true)
    );

    let moved_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx",
            "target_path":"row-moved.pptx",
            "slide_number":2,
            "table_number":1,
            "row":4,
            "expected_cells":["West","7","8","9"],
            "reference_row":2,
            "reference_expected_cells":["North","10","20","30"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect("move PPTX table row");
    assert_eq!(moved_row.get("moved_row").and_then(Value::as_u64), Some(2));
    assert_eq!(
        moved_row
            .get("row_xml_and_formatting_preserved")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after row move"),
        source_before
    );
    let row_moved_before = fs::read(root.join("row-moved.pptx")).expect("row-moved source bytes");
    let row_moved_table = presentation::inspect_pptx_table(
        &json!({"path":"row-moved.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect moved PPTX table row");
    for (row, expected) in ["Region", "West", "North", "South"].iter().enumerate() {
        assert_eq!(
            row_moved_table
                .pointer(format!("/cell_text/{row}/0").as_str())
                .and_then(Value::as_str),
            Some(*expected)
        );
    }
    let mut archive =
        ZipArchive::new(File::open(root.join("row-moved.pptx")).expect("row-moved PPTX file"))
            .expect("row-moved PPTX ZIP");
    let row_moved_slide =
        read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("row-moved slide XML");
    let mut height_cursor = 0usize;
    for height in ["1000000", "1454880", "1100000", "1200000"] {
        let marker = format!("<a:tr h=\"{height}\">");
        let offset = row_moved_slide[height_cursor..]
            .find(marker.as_str())
            .expect("moved row height in order");
        height_cursor += offset + marker.len();
    }

    let moved_column = presentation::move_pptx_table_column(
        &json!({
            "path":"row-moved.pptx",
            "target_path":"row-column-moved.pptx",
            "slide_number":2,
            "table_number":1,
            "column":4,
            "expected_cells":["Q3","9","30","3"],
            "reference_column":2,
            "reference_expected_cells":["Q1","7","10","1"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect("move PPTX table column");
    assert_eq!(
        moved_column.get("moved_column").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        moved_column
            .get("grid_column_and_cell_xml_preserved")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("row-moved.pptx")).expect("row source after column move"),
        row_moved_before
    );
    let moved_table = presentation::inspect_pptx_table(
        &json!({"path":"row-column-moved.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect moved PPTX table column");
    for (column, expected) in ["Region", "Q3", "Q1", "Q2"].iter().enumerate() {
        assert_eq!(
            moved_table
                .pointer(format!("/cell_text/0/{column}").as_str())
                .and_then(Value::as_str),
            Some(*expected)
        );
    }
    assert_eq!(
        moved_table
            .pointer("/cell_text/2/1")
            .and_then(Value::as_str),
        Some("30")
    );
    assert_eq!(
        moved_table
            .get("eligible_for_row_editing")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        moved_table
            .get("eligible_for_column_editing")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut archive = ZipArchive::new(
        File::open(root.join("row-column-moved.pptx")).expect("row-column-moved PPTX file"),
    )
    .expect("row-column-moved PPTX ZIP");
    let moved_slide =
        read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("row-column-moved slide XML");
    let mut width_cursor = 0usize;
    for width in ["1000000", "4820400", "2000000", "3000000"] {
        let marker = format!("<a:gridCol w=\"{width}\"/>");
        let offset = moved_slide[width_cursor..]
            .find(marker.as_str())
            .expect("moved grid-column width in order");
        width_cursor += offset + marker.len();
    }
    assert!(moved_slide.contains(concat!(
        "<a:rPr lang=\"zh-CN\" sz=\"1400\" b=\"1\">",
        "<a:solidFill><a:srgbClr val=\"1F2937\"/></a:solidFill>",
        "</a:rPr><a:t xml:space=\"preserve\">Q3</a:t>"
    )));

    presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"row-column-moved.pptx",
            "target_path":"edited-after-table-moves.pptx",
            "slide_number":2,"table_number":1,"row":3,"column":2,
            "expected_text":"30","replacement":"31"
        }),
        &state,
        &request,
    )
    .expect("edit cell after table moves");
    let final_table = presentation::inspect_pptx_table(
        &json!({"path":"edited-after-table-moves.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect table after move interoperability edit");
    assert_eq!(
        final_table
            .pointer("/cell_text/2/1")
            .and_then(Value::as_str),
        Some("31")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_stale_noop_unsafe_or_in_place_pptx_table_moves_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{
                "title":"Moves","layout":"table",
                "table":{"cells":[
                    ["Name","Value","Status"],
                    ["Alpha","1","Open"],
                    ["Beta","2","Closed"]
                ]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create table-move rejection fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("table-move source bytes");

    let stale_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"stale-row-move.pptx",
            "slide_number":1,"table_number":1,
            "row":3,"expected_cells":["Beta","3","Closed"],
            "reference_row":1,"reference_expected_cells":["Name","Value","Status"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("stale source row snapshot must fail");
    assert!(stale_row
        .to_string()
        .contains("does not match expected_cells"));

    let stale_reference_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"stale-reference-row.pptx",
            "slide_number":1,"table_number":1,
            "row":3,"expected_cells":["Beta","2","Closed"],
            "reference_row":1,"reference_expected_cells":["Name","Wrong","Status"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("stale reference row snapshot must fail");
    assert!(stale_reference_row
        .to_string()
        .contains("reference row does not match"));

    let same_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"same-row.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"expected_cells":["Alpha","1","Open"],
            "reference_row":2,"reference_expected_cells":["Alpha","1","Open"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("same source and reference row must fail");
    assert!(same_row.to_string().contains("must select different rows"));

    let noop_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"noop-row.pptx",
            "slide_number":1,"table_number":1,
            "row":1,"expected_cells":["Name","Value","Status"],
            "reference_row":2,"reference_expected_cells":["Alpha","1","Open"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("already adjacent row move must fail");
    assert!(noop_row
        .to_string()
        .contains("already in the requested position"));

    let in_place_row = presentation::move_pptx_table_row(
        &json!({
            "path":"source.pptx","target_path":"source.pptx","overwrite":true,
            "slide_number":1,"table_number":1,
            "row":3,"expected_cells":["Beta","2","Closed"],
            "reference_row":1,"reference_expected_cells":["Name","Value","Status"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("in-place row move must fail");
    assert!(in_place_row.to_string().contains("distinct target_path"));

    let stale_reference_column = presentation::move_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"stale-reference-column.pptx",
            "slide_number":1,"table_number":1,
            "column":3,"expected_cells":["Status","Open","Closed"],
            "reference_column":1,"reference_expected_cells":["Wrong","Alpha","Beta"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("stale reference column snapshot must fail");
    assert!(stale_reference_column
        .to_string()
        .contains("reference column does not match"));

    let same_column = presentation::move_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"same-column.pptx",
            "slide_number":1,"table_number":1,
            "column":2,"expected_cells":["Value","1","2"],
            "reference_column":2,"reference_expected_cells":["Value","1","2"],
            "position":"after"
        }),
        &state,
        &request,
    )
    .expect_err("same source and reference column must fail");
    assert!(same_column
        .to_string()
        .contains("must select different columns"));

    let noop_column = presentation::move_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"noop-column.pptx",
            "slide_number":1,"table_number":1,
            "column":1,"expected_cells":["Name","Alpha","Beta"],
            "reference_column":2,"reference_expected_cells":["Value","1","2"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("already adjacent column move must fail");
    assert!(noop_column
        .to_string()
        .contains("already in the requested position"));

    let in_place_column = presentation::move_pptx_table_column(
        &json!({
            "path":"source.pptx","target_path":"source.pptx","overwrite":true,
            "slide_number":1,"table_number":1,
            "column":3,"expected_cells":["Status","Open","Closed"],
            "reference_column":1,"reference_expected_cells":["Name","Alpha","Beta"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("in-place column move must fail");
    assert!(in_place_column.to_string().contains("distinct target_path"));

    presentation::create_pptx(
        &json!({
            "target_path":"attributed-row.pptx",
            "slides":[{"title":"Rows","layout":"table","table":{"cells":[["A","B"],["1","2"],["3","4"]]}}]
        }),
        &state,
        &request,
    )
    .expect("create attributed-row fixture");
    rewrite_zip_text_entry(
        root.join("attributed-row.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:tr h=\"", "<a:tr custom=\"1\" h=\"", 1),
    );
    let unsafe_row = presentation::move_pptx_table_row(
        &json!({
            "path":"attributed-row.pptx","target_path":"unsafe-row-move.pptx",
            "slide_number":1,"table_number":1,
            "row":3,"expected_cells":["3","4"],
            "reference_row":1,"reference_expected_cells":["A","B"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("attributed row movement must fail");
    assert!(unsafe_row.to_string().contains("canonical a:tr"));

    presentation::create_pptx(
        &json!({
            "target_path":"attributed-column.pptx",
            "slides":[{"title":"Columns","layout":"table","table":{"cells":[["A","B","C"],["1","2","3"]]}}]
        }),
        &state,
        &request,
    )
    .expect("create attributed-column fixture");
    rewrite_zip_text_entry(
        root.join("attributed-column.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:gridCol w=\"", "<a:gridCol custom=\"1\" w=\"", 1),
    );
    let unsafe_column = presentation::move_pptx_table_column(
        &json!({
            "path":"attributed-column.pptx","target_path":"unsafe-column-move.pptx",
            "slide_number":1,"table_number":1,
            "column":3,"expected_cells":["C","3"],
            "reference_column":1,"reference_expected_cells":["A","1"],
            "position":"before"
        }),
        &state,
        &request,
    )
    .expect_err("attributed column movement must fail");
    assert!(unsafe_column.to_string().contains("canonical a:gridCol"));

    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after rejected moves"),
        source_before
    );
    for path in [
        "stale-row-move.pptx",
        "stale-reference-row.pptx",
        "same-row.pptx",
        "noop-row.pptx",
        "stale-reference-column.pptx",
        "same-column.pptx",
        "noop-column.pptx",
        "unsafe-row-move.pptx",
        "unsafe-column-move.pptx",
    ] {
        assert!(
            !root.join(path).exists(),
            "unexpected table-move output: {path}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn copies_complete_pptx_table_cell_format_by_exact_xml_snapshots() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{
                "title":"Cell formatting","layout":"table",
                "table":{"cells":[
                    ["Name","Value","Status"],
                    ["Alpha","1","Open"],
                    ["Beta","2","Closed"]
                ]}
            }]
        }),
        &state,
        &request,
    )
    .expect("create cell-format fixture");
    rewrite_zip_text_entry(
        root.join("source.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| {
            let text = xml
                .find("<a:t xml:space=\"preserve\">Name</a:t>")
                .expect("reference cell text");
            let cell_start = xml[..text].rfind("<a:tc>").expect("reference cell start");
            let cell_end =
                text + xml[text..].find("</a:tc>").expect("reference cell end") + "</a:tc>".len();
            let cell = xml[cell_start..cell_end].replacen(
                "<a:pPr algn=\"l\"/>",
                "<a:pPr algn=\"ctr\"/>",
                1,
            );
            let cell = cell.replacen(
                "<a:solidFill><a:srgbClr val=\"1F2937\"/></a:solidFill>",
                "<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>",
                1,
            );
            let cell = cell.replacen(
                "<a:tcPr marL=\"45720\" marR=\"45720\" marT=\"22860\" marB=\"22860\" anchor=\"ctr\"/>",
                "<a:tcPr marL=\"45720\" marR=\"45720\" marT=\"22860\" marB=\"22860\" anchor=\"ctr\"><a:solidFill><a:srgbClr val=\"2563EB\"/></a:solidFill></a:tcPr>",
                1,
            );
            format!("{}{}{}", &xml[..cell_start], cell, &xml[cell_end..])
        },
    );
    let source_before = fs::read(root.join("source.pptx")).expect("cell-format source bytes");
    let source_table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect cell-format source");
    assert_eq!(
        source_table
            .get("eligible_for_cell_format_copy")
            .and_then(Value::as_bool),
        Some(true)
    );
    let target_hash = source_table
        .pointer("/cell_xml_sha256/1/0")
        .and_then(Value::as_str)
        .expect("target cell hash");
    let reference_hash = source_table
        .pointer("/cell_xml_sha256/0/0")
        .and_then(Value::as_str)
        .expect("reference cell hash");
    assert_eq!(target_hash.len(), 64);
    assert_eq!(reference_hash.len(), 64);
    assert_ne!(target_hash, reference_hash);

    let copied = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"formatted.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"Alpha",
            "expected_cell_xml_sha256":target_hash,
            "reference_row":1,"reference_column":1,"reference_expected_text":"Name",
            "reference_expected_cell_xml_sha256":reference_hash
        }),
        &state,
        &request,
    )
    .expect("copy complete PPTX table cell format");
    assert_eq!(
        copied.get("target_text_preserved").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        copied
            .get("reference_text_not_copied")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after cell-format copy"),
        source_before
    );
    let formatted = presentation::inspect_pptx_table(
        &json!({"path":"formatted.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect formatted PPTX table");
    assert_eq!(
        formatted.pointer("/cell_text/1/0").and_then(Value::as_str),
        Some("Alpha")
    );
    assert_ne!(
        formatted
            .pointer("/cell_xml_sha256/1/0")
            .and_then(Value::as_str),
        Some(target_hash)
    );
    assert_eq!(
        formatted
            .pointer("/cell_xml_sha256/1/0")
            .and_then(Value::as_str),
        copied.get("cell_xml_sha256").and_then(Value::as_str)
    );
    let mut archive =
        ZipArchive::new(File::open(root.join("formatted.pptx")).expect("formatted PPTX file"))
            .expect("formatted PPTX ZIP");
    let formatted_slide =
        read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("formatted slide XML");
    assert_eq!(
        formatted_slide
            .matches("<a:solidFill><a:srgbClr val=\"2563EB\"/></a:solidFill>")
            .count(),
        2
    );
    assert_eq!(formatted_slide.matches("<a:pPr algn=\"ctr\"/>").count(), 2);
    assert_eq!(
        formatted_slide
            .matches("<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>")
            .count(),
        2
    );
    assert!(formatted_slide.contains(concat!(
        "<a:rPr lang=\"zh-CN\" sz=\"1400\" b=\"1\">",
        "<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>",
        "</a:rPr><a:t xml:space=\"preserve\">Alpha</a:t>"
    )));
    assert!(formatted_slide.contains(concat!(
        "<a:rPr lang=\"zh-CN\" sz=\"1400\" b=\"0\">",
        "<a:solidFill><a:srgbClr val=\"1F2937\"/></a:solidFill>",
        "</a:rPr><a:t xml:space=\"preserve\">1</a:t>"
    )));

    presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"formatted.pptx","target_path":"formatted-edited.pptx",
            "slide_number":1,"table_number":1,"row":2,"column":1,
            "expected_text":"Alpha","replacement":"Alpha Prime"
        }),
        &state,
        &request,
    )
    .expect("replace text after cell-format copy");
    let mut archive = ZipArchive::new(
        File::open(root.join("formatted-edited.pptx")).expect("formatted-edited PPTX file"),
    )
    .expect("formatted-edited PPTX ZIP");
    let edited_slide =
        read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("edited formatted slide XML");
    assert!(edited_slide.contains(concat!(
        "<a:rPr lang=\"zh-CN\" sz=\"1400\" b=\"1\">",
        "<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>",
        "</a:rPr><a:t xml:space=\"preserve\">Alpha Prime</a:t>"
    )));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_stale_noop_unsafe_or_in_place_pptx_table_cell_format_copy_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"Formats","layout":"table","table":{"cells":[
                ["Name","Value"],["Alpha","1"],["Beta","2"]
            ]}}]
        }),
        &state,
        &request,
    )
    .expect("create rejected cell-format fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("cell-format source bytes");
    let table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":1,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect rejected cell-format source");
    let header_hash = table
        .pointer("/cell_xml_sha256/0/0")
        .and_then(Value::as_str)
        .expect("header hash");
    let alpha_hash = table
        .pointer("/cell_xml_sha256/1/0")
        .and_then(Value::as_str)
        .expect("alpha hash");
    let alpha_value_hash = table
        .pointer("/cell_xml_sha256/1/1")
        .and_then(Value::as_str)
        .expect("alpha value hash");
    let beta_value_hash = table
        .pointer("/cell_xml_sha256/2/1")
        .and_then(Value::as_str)
        .expect("beta value hash");

    let stale_target = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"stale-target.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"Alpha",
            "expected_cell_xml_sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "reference_row":1,"reference_column":1,"reference_expected_text":"Name",
            "reference_expected_cell_xml_sha256":header_hash
        }),
        &state,
        &request,
    )
    .expect_err("stale target cell hash must fail");
    assert!(stale_target
        .to_string()
        .contains("target PPTX table cell XML"));

    let stale_reference = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"stale-reference.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"Alpha",
            "expected_cell_xml_sha256":alpha_hash,
            "reference_row":1,"reference_column":1,"reference_expected_text":"Wrong",
            "reference_expected_cell_xml_sha256":header_hash
        }),
        &state,
        &request,
    )
    .expect_err("stale reference text must fail");
    assert!(stale_reference
        .to_string()
        .contains("reference_expected_text"));

    let same_cell = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"same-cell.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"Alpha",
            "expected_cell_xml_sha256":alpha_hash,
            "reference_row":2,"reference_column":1,"reference_expected_text":"Alpha",
            "reference_expected_cell_xml_sha256":alpha_hash
        }),
        &state,
        &request,
    )
    .expect_err("same target and reference cell must fail");
    assert!(same_cell
        .to_string()
        .contains("different target and reference"));

    let noop = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"noop.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":2,"expected_text":"1",
            "expected_cell_xml_sha256":alpha_value_hash,
            "reference_row":3,"reference_column":2,"reference_expected_text":"2",
            "reference_expected_cell_xml_sha256":beta_value_hash
        }),
        &state,
        &request,
    )
    .expect_err("identical target and reference formatting must fail");
    assert!(noop
        .to_string()
        .contains("already has the reference cell formatting"));

    let in_place = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"source.pptx","target_path":"source.pptx","overwrite":true,
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"Alpha",
            "expected_cell_xml_sha256":alpha_hash,
            "reference_row":1,"reference_column":1,"reference_expected_text":"Name",
            "reference_expected_cell_xml_sha256":header_hash
        }),
        &state,
        &request,
    )
    .expect_err("in-place cell-format copy must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    presentation::create_pptx(
        &json!({
            "target_path":"attributed-cell.pptx",
            "slides":[{"title":"Unsafe","layout":"table","table":{"cells":[["A","B"],["1","2"]]}}]
        }),
        &state,
        &request,
    )
    .expect("create attributed-cell fixture");
    rewrite_zip_text_entry(
        root.join("attributed-cell.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:tc>", "<a:tc custom=\"1\">", 1),
    );
    let unsafe_copy = presentation::copy_pptx_table_cell_format(
        &json!({
            "path":"attributed-cell.pptx","target_path":"unsafe-copy.pptx",
            "slide_number":1,"table_number":1,
            "row":2,"column":1,"expected_text":"1",
            "expected_cell_xml_sha256":alpha_hash,
            "reference_row":1,"reference_column":1,"reference_expected_text":"A",
            "reference_expected_cell_xml_sha256":header_hash
        }),
        &state,
        &request,
    )
    .expect_err("attributed PPTX table cell must fail closed");
    assert!(unsafe_copy.to_string().contains("not eligible"));

    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after rejected cell-format copies"),
        source_before
    );
    for path in [
        "stale-target.pptx",
        "stale-reference.pptx",
        "same-cell.pptx",
        "noop.pptx",
        "unsafe-copy.pptx",
    ] {
        assert!(
            !root.join(path).exists(),
            "unexpected cell-format output: {path}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspects_standard_pptx_charts_by_visible_order_without_opening_embedded_workbooks() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"charted.pptx",
            "slides":[
                {"title":"Overview","body":"No chart"},
                {"title":"Sales","body":"Chart follows"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create chart inspection fixture");
    add_standard_pptx_chart_fixture(root.join("charted.pptx").as_path(), 2);
    let source_before = fs::read(root.join("charted.pptx")).expect("charted source bytes");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"charted.pptx",
            "target_path":"reordered-charted.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder charted deck");
    assert_eq!(
        fs::read(root.join("charted.pptx")).expect("charted source after reorder"),
        source_before
    );

    let inspected = presentation::inspect_pptx_charts(
        &json!({"path":"reordered-charted.pptx","slide_numbers":[1]}),
        &state,
        &request,
    )
    .expect("inspect standard PPTX chart");
    assert_eq!(inspected.get("charts").and_then(Value::as_u64), Some(1));
    assert_eq!(
        inspected.get("selected_charts").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/slide_number")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/part")
            .and_then(Value::as_str),
        Some("ppt/charts/chart1.xml")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/title")
            .and_then(Value::as_str),
        Some("Quarterly Sales")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/chart_types/0")
            .and_then(Value::as_str),
        Some("bar")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/series_count")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/cached_points")
            .and_then(Value::as_u64),
        Some(12)
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/series/0/name")
            .and_then(Value::as_str),
        Some("North")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/series/0/category_formula")
            .and_then(Value::as_str),
        Some("Sheet1!$A$2:$A$4")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/series/1/values_preview/2")
            .and_then(Value::as_str),
        Some("36")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/data_source")
            .and_then(Value::as_str),
        Some("cached_with_embedded_workbook")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/embedded_workbook")
            .and_then(Value::as_str),
        Some("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx")
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/chart_xml_sha256")
            .and_then(Value::as_str)
            .map(str::len),
        Some(64)
    );
    assert_eq!(
        inspected
            .get("embedded_workbooks_opened")
            .and_then(Value::as_bool),
        Some(false)
    );
    let unselected = presentation::inspect_pptx_charts(
        &json!({"path":"reordered-charted.pptx","slide_numbers":[2]}),
        &state,
        &request,
    )
    .expect("inspect slide without chart");
    assert_eq!(unselected.get("charts").and_then(Value::as_u64), Some(1));
    assert_eq!(
        unselected.get("selected_charts").and_then(Value::as_u64),
        Some(0)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_external_unreferenced_or_extended_pptx_charts() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"charted.pptx",
            "slides":[{"title":"Chart","body":"Safe cached chart"}]
        }),
        &state,
        &request,
    )
    .expect("create rejected chart fixture");
    add_standard_pptx_chart_fixture(root.join("charted.pptx").as_path(), 1);

    fs::copy(root.join("charted.pptx"), root.join("external-chart.pptx"))
        .expect("copy external chart fixture");
    rewrite_zip_text_entry(
        root.join("external-chart.pptx").as_path(),
        "ppt/slides/_rels/slide1.xml.rels",
        |xml| {
            xml.replacen(
                "Target=\"../charts/chart1.xml\"/>",
                "Target=\"https://example.com/chart.xml\" TargetMode=\"External\"/>",
                1,
            )
        },
    );
    let external =
        presentation::inspect_pptx_charts(&json!({"path":"external-chart.pptx"}), &state, &request)
            .expect_err("external chart relationship must fail closed");
    assert!(external.to_string().contains("must be internal"));

    fs::copy(
        root.join("charted.pptx"),
        root.join("unreferenced-chart.pptx"),
    )
    .expect("copy unreferenced chart fixture");
    add_zip_entries(
        root.join("unreferenced-chart.pptx").as_path(),
        &[(
            "ppt/charts/chart2.xml",
            b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/>",
        )],
    );
    let unreferenced = presentation::inspect_pptx_charts(
        &json!({"path":"unreferenced-chart.pptx"}),
        &state,
        &request,
    )
    .expect_err("unreferenced chart part must fail closed");
    assert!(unreferenced.to_string().contains("unreferenced or missing"));

    fs::copy(root.join("charted.pptx"), root.join("extended-chart.pptx"))
        .expect("copy chartEx fixture");
    add_zip_entries(
        root.join("extended-chart.pptx").as_path(),
        &[(
            "ppt/charts/chartEx1.xml",
            b"<cx:chartSpace xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"/>",
        )],
    );
    let extended =
        presentation::inspect_pptx_charts(&json!({"path":"extended-chart.pptx"}), &state, &request)
            .expect_err("chartEx package must fail closed");
    assert!(extended.to_string().contains("does not support chartEx"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_and_appends_self_contained_standard_pptx_charts_without_workbooks() {
    let (root, state, request) = test_context();
    let created = presentation::create_pptx(
        &json!({
            "target_path":"charts.pptx",
            "slides":[
                {
                    "title":"Quarterly revenue",
                    "layout":"chart",
                    "chart":{
                        "type":"column",
                        "title":"Revenue by region",
                        "categories":["Q1","Q2","Q3"],
                        "series":[
                            {"name":"North","values":[10,20,30]},
                            {"name":"South","values":[12.5,18,36],"value_axis":"secondary"}
                        ],
                        "legend_position":"top",
                        "data_labels":"value",
                        "category_axis_title":"Quarter",
                        "value_axis_title":"Revenue",
                        "secondary_value_axis_title":"Growth",
                        "value_axis_minimum":1,
                        "value_axis_maximum":40,
                        "value_axis_log_base":10,
                        "value_axis_major_tick_mark":"inside",
                        "value_axis_minor_tick_mark":"outside",
                        "value_axis_major_unit":10,
                        "value_axis_minor_unit":2,
                        "value_axis_number_format":"thousands_2",
                        "secondary_value_axis_minimum":1,
                        "secondary_value_axis_maximum":50,
                        "secondary_value_axis_log_base":2,
                        "secondary_value_axis_major_tick_mark":"cross",
                        "secondary_value_axis_minor_tick_mark":"inside",
                        "secondary_value_axis_major_unit":10,
                        "secondary_value_axis_minor_unit":2.5,
                        "secondary_value_axis_number_format":"decimal_1"
                    }
                },
                {
                    "title":"Retention trend",
                    "layout":"chart",
                    "chart":{
                        "type":"line",
                        "categories":["Jan","Feb","Mar"],
                        "series":[{"name":"Retention","values":[91,92.5,94]}],
                        "show_legend":false
                    }
                },
                {
                    "title":"Channel mix",
                    "layout":"chart",
                    "chart":{
                        "type":"pie",
                        "title":"Current mix",
                        "categories":["Direct","Partner","Organic"],
                        "series":[{"name":"Share","values":[45,30,25]}]
                    }
                },
                {
                    "title":"Capacity range",
                    "layout":"chart",
                    "chart":{
                        "type":"area",
                        "title":"Capacity by tier",
                        "categories":["Base","Peak","Burst"],
                        "series":[
                            {"name":"Committed","values":[20,35,-5]},
                            {"name":"Available","values":[30,45,15]}
                        ]
                    }
                },
                {
                    "title":"Portfolio allocation",
                    "layout":"chart",
                    "chart":{
                        "type":"doughnut",
                        "title":"Current allocation",
                        "categories":["Core","Growth","Reserve"],
                        "series":[{"name":"Allocation","values":[60,30,10]}],
                        "legend_position":"bottom",
                        "data_labels":"percentage"
                    }
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("create self-contained chart deck");
    assert_eq!(created.get("charts").and_then(Value::as_u64), Some(5));
    assert_eq!(
        created.get("chart_types"),
        Some(&json!(["column", "line", "pie", "area", "doughnut"]))
    );

    let inspected =
        presentation::inspect_pptx_charts(&json!({"path":"charts.pptx"}), &state, &request)
            .expect("inspect generated chart deck");
    assert_eq!(inspected.get("charts").and_then(Value::as_u64), Some(5));
    assert_eq!(
        inspected.pointer("/chart_metadata/0/chart_types/0"),
        Some(&json!("bar"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/title"),
        Some(&json!("Revenue by region"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/series/1/name"),
        Some(&json!("South"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/series/1/values_preview/0"),
        Some(&json!("12.5"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/series/1/value_axis"),
        Some(&json!("secondary"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/chart_group_count"),
        Some(&json!(2))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/axis_count"),
        Some(&json!(4))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/series/0/name_formula"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/data_source"),
        Some(&json!("cached_only"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/relationship_count"),
        Some(&json!(0))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/embedded_workbook"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/legend_position"),
        Some(&json!("top"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/data_labels"),
        Some(&json!("value"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/category_axis_title"),
        Some(&json!("Quarter"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_title"),
        Some(&json!("Revenue"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_title"),
        Some(&json!("Growth"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_minimum"),
        Some(&json!("1"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_maximum"),
        Some(&json!("40"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_log_base"),
        Some(&json!("10"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&json!("inside"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_major_tick_mark_value"),
        Some(&json!("in"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_minor_tick_mark"),
        Some(&json!("outside"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_minor_tick_mark_value"),
        Some(&json!("out"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&json!("10"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_minor_unit"),
        Some(&json!("2"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_number_format"),
        Some(&json!("thousands_2"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_number_format_code"),
        Some(&json!("#,##0.00"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minimum"),
        Some(&json!("1"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_maximum"),
        Some(&json!("50"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_log_base"),
        Some(&json!("2"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_major_tick_mark"),
        Some(&json!("cross"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_major_tick_mark_value"),
        Some(&json!("cross"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minor_tick_mark"),
        Some(&json!("inside"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minor_tick_mark_value"),
        Some(&json!("in"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_major_unit"),
        Some(&json!("10"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minor_unit"),
        Some(&json!("2.5"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_number_format"),
        Some(&json!("decimal_1"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_axis_series"),
        Some(&json!([2]))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/self_contained_edit_snapshot/series/0/value_axis"),
        Some(&json!("primary"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/self_contained_edit_snapshot/series/1/value_axis"),
        Some(&json!("secondary"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_minimum"),
        Some(&json!(1.0))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_log_base"),
        Some(&json!(10.0))
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_major_tick_mark"),
        Some(&json!("inside"))
    );
    assert_eq!(
        inspected.pointer(
            "/chart_metadata/0/self_contained_edit_snapshot/secondary_value_axis_minor_tick_mark"
        ),
        Some(&json!("inside"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_major_unit"),
        Some(&json!(10.0))
    );
    assert_eq!(
        inspected.pointer(
            "/chart_metadata/0/self_contained_edit_snapshot/secondary_value_axis_number_format"
        ),
        Some(&json!("decimal_1"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/1/chart_types/0"),
        Some(&json!("line"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/1/legend_position"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/1/self_contained_edit_snapshot/legend_position"),
        Some(&json!("right"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/1/self_contained_edit_snapshot/data_labels"),
        Some(&json!("none"))
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/1/self_contained_edit_snapshot/value_axis_major_tick_mark"),
        Some(&json!("none"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/2/chart_types/0"),
        Some(&json!("pie"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/3/chart_types/0"),
        Some(&json!("area"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/3/series/0/values_preview/2"),
        Some(&json!("-5"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/4/chart_types/0"),
        Some(&json!("doughnut"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/4/legend_position"),
        Some(&json!("bottom"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/4/data_labels"),
        Some(&json!("percentage"))
    );
    for index in 0..5 {
        assert_eq!(
            inspected.pointer(&format!(
                "/chart_metadata/{index}/eligible_for_self_contained_chart_replacement"
            )),
            Some(&json!(true))
        );
        assert!(inspected
            .pointer(&format!(
                "/chart_metadata/{index}/self_contained_edit_snapshot"
            ))
            .is_some_and(|value| !value.is_null()));
    }
    let mut created_archive =
        ZipArchive::new(File::open(root.join("charts.pptx")).expect("chart deck file"))
            .expect("chart deck ZIP");
    assert!((0..created_archive.len()).all(|index| {
        !created_archive
            .by_index(index)
            .expect("chart deck entry")
            .name()
            .starts_with("ppt/embeddings/")
    }));
    let first_chart =
        read_zip_text(&mut created_archive, "ppt/charts/chart1.xml").expect("generated chart XML");
    assert!(first_chart.contains("<c:tx><c:v>North</c:v></c:tx>"));
    assert!(first_chart.contains("<c:strLit>"));
    assert!(first_chart.contains("<c:numLit>"));
    assert!(!first_chart.contains("<c:strRef>"));
    assert!(!first_chart.contains("<c:numRef>"));
    assert!(!first_chart.contains("<c:externalData"));
    assert!(first_chart.contains("<c:legendPos val=\"t\"/>"));
    assert!(first_chart.contains("<c:showVal val=\"1\"/>"));
    assert!(first_chart.contains("Quarter"));
    assert!(first_chart.contains("Revenue"));
    assert_eq!(first_chart.matches("<c:barChart>").count(), 2);
    assert_eq!(first_chart.matches("<c:dLbls>").count(), 2);
    assert!(first_chart.contains("<c:axPos val=\"r\"/>"));
    assert!(first_chart.contains("<c:delete val=\"1\"/><c:axPos val=\"t\"/>"));
    assert!(first_chart.contains("Growth"));
    assert!(first_chart.contains(
        "<c:scaling><c:logBase val=\"10\"/><c:orientation val=\"minMax\"/><c:max val=\"40\"/><c:min val=\"1\"/></c:scaling>"
    ));
    assert!(first_chart.contains("<c:numFmt formatCode=\"#,##0.00\" sourceLinked=\"0\"/>"));
    assert!(first_chart.contains(
        "<c:numFmt formatCode=\"#,##0.00\" sourceLinked=\"0\"/><c:majorTickMark val=\"in\"/><c:minorTickMark val=\"out\"/><c:tickLblPos val=\"nextTo\"/>"
    ));
    assert!(first_chart.contains("<c:majorUnit val=\"10\"/><c:minorUnit val=\"2\"/>"));
    assert!(first_chart.contains(
        "<c:scaling><c:logBase val=\"2\"/><c:orientation val=\"minMax\"/><c:max val=\"50\"/><c:min val=\"1\"/></c:scaling>"
    ));
    assert!(first_chart.contains("<c:numFmt formatCode=\"0.0\" sourceLinked=\"0\"/>"));
    assert!(first_chart.contains(
        "<c:numFmt formatCode=\"0.0\" sourceLinked=\"0\"/><c:majorTickMark val=\"cross\"/><c:minorTickMark val=\"in\"/><c:tickLblPos val=\"nextTo\"/>"
    ));
    assert!(first_chart.contains("<c:majorUnit val=\"10\"/><c:minorUnit val=\"2.5\"/>"));
    let area_chart =
        read_zip_text(&mut created_archive, "ppt/charts/chart4.xml").expect("generated area XML");
    assert!(area_chart.contains("<c:areaChart>"));
    assert!(area_chart.contains("<c:catAx>"));
    assert!(area_chart.contains("<c:valAx>"));
    assert!(area_chart.contains("<c:crossBetween val=\"midCat\"/>"));
    let doughnut_chart = read_zip_text(&mut created_archive, "ppt/charts/chart5.xml")
        .expect("generated doughnut XML");
    assert!(doughnut_chart.contains("<c:doughnutChart>"));
    assert!(doughnut_chart.contains("<c:holeSize val=\"50\"/>"));
    assert!(doughnut_chart.contains("<c:legendPos val=\"b\"/>"));
    assert!(doughnut_chart.contains("<c:showPercent val=\"1\"/>"));
    assert!(!doughnut_chart.contains("<c:catAx>"));
    drop(created_archive);

    let source_before = fs::read(root.join("charts.pptx")).expect("chart source bytes");
    let appended = presentation::append_pptx_slides(
        &json!({
            "path":"charts.pptx",
            "target_path":"appended-charts.pptx",
            "slides":[
                {
                    "title":"Forecast range",
                    "layout":"chart",
                    "chart":{
                        "type":"area",
                        "categories":["Apr","May"],
                        "series":[
                            {"name":"Forecast","values":[40,-5]},
                            {"name":"Actual","values":[35,10],"value_axis":"secondary"}
                        ],
                        "legend_position":"left",
                        "data_labels":"value",
                        "category_axis_title":"Month",
                        "value_axis_title":"Units",
                        "secondary_value_axis_title":"Variance",
                        "value_axis_minimum":-10,
                        "value_axis_maximum":50,
                        "value_axis_major_tick_mark":"outside",
                        "value_axis_minor_tick_mark":"cross",
                        "value_axis_major_unit":20,
                        "value_axis_minor_unit":5,
                        "value_axis_number_format":"integer",
                        "secondary_value_axis_minimum":1,
                        "secondary_value_axis_maximum":40,
                        "secondary_value_axis_log_base":10,
                        "secondary_value_axis_major_tick_mark":"inside",
                        "secondary_value_axis_minor_tick_mark":"outside",
                        "secondary_value_axis_major_unit":10,
                        "secondary_value_axis_minor_unit":2,
                        "secondary_value_axis_number_format":"decimal_2"
                    }
                },
                {
                    "title":"Forecast mix",
                    "layout":"chart",
                    "chart":{
                        "type":"doughnut",
                        "categories":["Committed","Uncommitted"],
                        "series":[{"name":"Share","values":[75,25]}],
                        "data_labels":"percentage"
                    }
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("append self-contained chart slide");
    assert_eq!(
        appended.get("appended_charts").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        appended.get("appended_chart_types"),
        Some(&json!(["area", "doughnut"]))
    );
    assert_eq!(
        fs::read(root.join("charts.pptx")).expect("source after chart append"),
        source_before
    );
    let appended_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"appended-charts.pptx","slide_numbers":[6,7]}),
        &state,
        &request,
    )
    .expect("inspect appended chart");
    assert_eq!(
        appended_inspection
            .get("selected_charts")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/part"),
        Some(&json!("ppt/charts/chart6.xml"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/chart_types/0"),
        Some(&json!("area"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/series/0/values_preview/1"),
        Some(&json!("-5"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/legend_position"),
        Some(&json!("left"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/category_axis_title"),
        Some(&json!("Month"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/series/1/value_axis"),
        Some(&json!("secondary"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_title"),
        Some(&json!("Variance"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_minimum"),
        Some(&json!("-10"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_number_format"),
        Some(&json!("integer"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&json!("outside"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_minor_tick_mark_value"),
        Some(&json!("cross"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&json!("20"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/value_axis_minor_unit"),
        Some(&json!("5"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_maximum"),
        Some(&json!("40"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_log_base"),
        Some(&json!("10"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_major_tick_mark"),
        Some(&json!("inside"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_minor_tick_mark_value"),
        Some(&json!("out"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_number_format"),
        Some(&json!("decimal_2"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/0/secondary_value_axis_major_unit"),
        Some(&json!("10"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/1/part"),
        Some(&json!("ppt/charts/chart7.xml"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/1/chart_types/0"),
        Some(&json!("doughnut"))
    );
    assert_eq!(
        appended_inspection.pointer("/chart_metadata/1/data_labels"),
        Some(&json!("percentage"))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_canonical_self_contained_pptx_chart_without_modifying_source_or_relationships() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"replace-chart-source.pptx",
            "slides":[{
                "title":"Revenue",
                "layout":"chart",
                "chart":{
                    "type":"area",
                    "title":"Quarterly revenue",
                    "categories":["Q1","Q2"],
                    "series":[
                        {"name":"North","values":[10,20]},
                        {"name":"South","values":[12,18],"value_axis":"secondary"}
                    ],
                    "show_legend":true,
                    "legend_position":"left",
                    "data_labels":"value",
                    "category_axis_title":"Quarter",
                    "value_axis_title":"Revenue",
                    "secondary_value_axis_title":"Margin",
                    "value_axis_minimum":1,
                    "value_axis_maximum":25,
                    "value_axis_log_base":10,
                    "value_axis_major_tick_mark":"inside",
                    "value_axis_minor_tick_mark":"outside",
                    "value_axis_major_unit":5,
                    "value_axis_minor_unit":1,
                    "value_axis_number_format":"thousands",
                    "secondary_value_axis_minimum":1,
                    "secondary_value_axis_maximum":20,
                    "secondary_value_axis_log_base":2,
                    "secondary_value_axis_major_tick_mark":"cross",
                    "secondary_value_axis_minor_tick_mark":"inside",
                    "secondary_value_axis_major_unit":5,
                    "secondary_value_axis_minor_unit":1,
                    "secondary_value_axis_number_format":"decimal_2"
                }
            }]
        }),
        &state,
        &request,
    )
    .expect("create canonical chart source");
    let source = root.join("replace-chart-source.pptx");
    let source_before = fs::read(source.as_path()).expect("chart source bytes");
    let inspected = presentation::inspect_pptx_charts(
        &json!({"path":"replace-chart-source.pptx"}),
        &state,
        &request,
    )
    .expect("inspect canonical chart source");
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspected
            .pointer("/chart_metadata/0/show_legend")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/legend_position"),
        Some(&json!("left"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/data_labels"),
        Some(&json!("value"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_axis_series"),
        Some(&json!([2]))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_title"),
        Some(&json!("Margin"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_number_format"),
        Some(&json!("thousands"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_maximum"),
        Some(&json!("20"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_log_base"),
        Some(&json!("10"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_log_base"),
        Some(&json!("2"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&json!("inside"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minor_tick_mark_value"),
        Some(&json!("in"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&json!("5"))
    );
    assert_eq!(
        inspected.pointer("/chart_metadata/0/secondary_value_axis_minor_unit"),
        Some(&json!("1"))
    );
    let hash = inspected
        .pointer("/chart_metadata/0/chart_xml_sha256")
        .and_then(Value::as_str)
        .expect("chart hash")
        .to_string();
    let snapshot = inspected
        .pointer("/chart_metadata/0/self_contained_edit_snapshot")
        .expect("canonical edit snapshot")
        .clone();

    let (source_slide, source_relationships, source_content_types, source_chart) = {
        let mut archive = ZipArchive::new(File::open(source.as_path()).expect("chart source file"))
            .expect("chart source ZIP");
        (
            read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("source slide"),
            read_zip_text(&mut archive, "ppt/slides/_rels/slide1.xml.rels")
                .expect("source slide relationships"),
            read_zip_text(&mut archive, "[Content_Types].xml").expect("source content types"),
            read_zip_text(&mut archive, "ppt/charts/chart1.xml").expect("source chart"),
        )
    };
    let replacement = json!({
        "type":"doughnut",
        "title":"Channel mix",
        "categories":["Direct","Partner","Organic"],
        "series":[{"name":"Share","values":[55,30,15]}],
        "show_legend":true,
        "legend_position":"bottom",
        "data_labels":"percentage"
    });
    let updated = presentation::replace_pptx_chart(
        &json!({
            "path":"replace-chart-source.pptx",
            "target_path":"replace-chart-output.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":hash,
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":replacement
        }),
        &state,
        &request,
    )
    .expect("replace canonical self-contained chart");
    assert_eq!(updated.get("operation"), Some(&json!("replace_chart")));
    assert_eq!(updated.get("part"), Some(&json!("ppt/charts/chart1.xml")));
    assert_ne!(
        updated.get("previous_chart_xml_sha256"),
        updated.get("chart_xml_sha256")
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after chart replacement"),
        source_before
    );

    let output = root.join("replace-chart-output.pptx");
    let mut archive = ZipArchive::new(File::open(output.as_path()).expect("chart output file"))
        .expect("chart output ZIP");
    assert_eq!(
        read_zip_text(&mut archive, "ppt/slides/slide1.xml").expect("output slide"),
        source_slide
    );
    assert_eq!(
        read_zip_text(&mut archive, "ppt/slides/_rels/slide1.xml.rels")
            .expect("output slide relationships"),
        source_relationships
    );
    assert_eq!(
        read_zip_text(&mut archive, "[Content_Types].xml").expect("output content types"),
        source_content_types
    );
    let output_chart = read_zip_text(&mut archive, "ppt/charts/chart1.xml").expect("output chart");
    assert_ne!(output_chart, source_chart);
    assert!(output_chart.contains("<c:doughnutChart>"));
    assert!(output_chart.contains("<c:holeSize val=\"50\"/>"));
    assert!(output_chart.contains("<c:title>"));
    assert!(output_chart.contains("Channel mix"));
    assert!(output_chart.contains("<c:legendPos val=\"b\"/>"));
    assert!(output_chart.contains("<c:showPercent val=\"1\"/>"));
    assert!(!output_chart.contains("<c:axPos val=\"r\"/>"));
    assert!(!output_chart.contains("<c:majorTickMark"));
    assert!(!output_chart.contains("<c:minorTickMark"));
    assert!(archive.by_name("ppt/charts/_rels/chart1.xml.rels").is_err());
    drop(archive);

    let inspected_output = presentation::inspect_pptx_charts(
        &json!({"path":"replace-chart-output.pptx"}),
        &state,
        &request,
    )
    .expect("inspect replaced chart");
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/chart_types/0"),
        Some(&json!("doughnut"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/title"),
        Some(&json!("Channel mix"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/show_legend"),
        Some(&json!(true))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/legend_position"),
        Some(&json!("bottom"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/data_labels"),
        Some(&json!("percentage"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/series/0/values_preview/2"),
        Some(&json!("15"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/value_axis_minimum"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/value_axis_log_base"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected_output
            .pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_major_tick_mark"),
        Some(&json!("none"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&Value::Null)
    );
    assert_eq!(
        inspected_output
            .pointer("/chart_metadata/0/self_contained_edit_snapshot/value_axis_number_format"),
        Some(&json!("general"))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(true))
    );
    assert_eq!(
        inspected_output.pointer("/chart_metadata/0/chart_xml_sha256"),
        updated.get("chart_xml_sha256")
    );

    let output_before_second_replacement =
        fs::read(output.as_path()).expect("doughnut chart output bytes");
    let doughnut_hash = inspected_output
        .pointer("/chart_metadata/0/chart_xml_sha256")
        .and_then(Value::as_str)
        .expect("doughnut chart hash");
    let doughnut_snapshot = inspected_output
        .pointer("/chart_metadata/0/self_contained_edit_snapshot")
        .expect("doughnut chart snapshot");
    presentation::replace_pptx_chart(
        &json!({
            "path":"replace-chart-output.pptx",
            "target_path":"replace-chart-area-output.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":doughnut_hash,
            "expected_self_contained_edit_snapshot":doughnut_snapshot,
            "replacement":{
                "type":"area",
                "title":"Channel trend",
                "categories":["Direct","Partner","Organic"],
                "series":[
                    {"name":"Current","values":[55,30,15]},
                    {"name":"Previous","values":[50,35,15],"value_axis":"secondary"}
                ],
                "show_legend":true,
                "legend_position":"top",
                "data_labels":"value",
                "category_axis_title":"Channel",
                "value_axis_title":"Share",
                "secondary_value_axis_title":"Prior share",
                "value_axis_minimum":1,
                "value_axis_maximum":60,
                "value_axis_log_base":10,
                "value_axis_major_tick_mark":"cross",
                "value_axis_minor_tick_mark":"inside",
                "value_axis_major_unit":20,
                "value_axis_minor_unit":5,
                "value_axis_number_format":"decimal_1",
                "secondary_value_axis_minimum":1,
                "secondary_value_axis_maximum":60,
                "secondary_value_axis_log_base":2,
                "secondary_value_axis_major_tick_mark":"outside",
                "secondary_value_axis_minor_tick_mark":"cross",
                "secondary_value_axis_major_unit":15,
                "secondary_value_axis_minor_unit":5,
                "secondary_value_axis_number_format":"integer"
            }
        }),
        &state,
        &request,
    )
    .expect("replace canonical doughnut chart with area chart");
    assert_eq!(
        fs::read(output.as_path()).expect("doughnut source after second replacement"),
        output_before_second_replacement
    );
    let inspected_area = presentation::inspect_pptx_charts(
        &json!({"path":"replace-chart-area-output.pptx"}),
        &state,
        &request,
    )
    .expect("inspect second area replacement");
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/chart_types/0"),
        Some(&json!("area"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/series/1/name"),
        Some(&json!("Previous"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/legend_position"),
        Some(&json!("top"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/data_labels"),
        Some(&json!("value"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/category_axis_title"),
        Some(&json!("Channel"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_title"),
        Some(&json!("Share"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/series/1/value_axis"),
        Some(&json!("secondary"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_title"),
        Some(&json!("Prior share"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_maximum"),
        Some(&json!("60"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_number_format"),
        Some(&json!("decimal_1"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_log_base"),
        Some(&json!("10"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&json!("cross"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_minor_tick_mark_value"),
        Some(&json!("in"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&json!("20"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/value_axis_minor_unit"),
        Some(&json!("5"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_number_format"),
        Some(&json!("integer"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_log_base"),
        Some(&json!("2"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_major_tick_mark"),
        Some(&json!("outside"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_minor_tick_mark_value"),
        Some(&json!("cross"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/secondary_value_axis_major_unit"),
        Some(&json!("15"))
    );
    assert_eq!(
        inspected_area.pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(true))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_stale_unsafe_or_noncanonical_pptx_chart_replacements_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"chart-edit-source.pptx",
            "slides":[{
                "title":"Trend",
                "layout":"chart",
                "chart":{
                    "type":"line",
                    "title":"Retention",
                    "categories":["Jan","Feb"],
                    "series":[{"name":"Rate","values":[91,93]}],
                    "show_legend":false
                }
            }]
        }),
        &state,
        &request,
    )
    .expect("create chart edit rejection source");
    let source = root.join("chart-edit-source.pptx");
    let source_before = fs::read(source.as_path()).expect("chart edit source bytes");
    let inspected = presentation::inspect_pptx_charts(
        &json!({"path":"chart-edit-source.pptx"}),
        &state,
        &request,
    )
    .expect("inspect chart edit source");
    let hash = inspected
        .pointer("/chart_metadata/0/chart_xml_sha256")
        .and_then(Value::as_str)
        .expect("chart hash")
        .to_string();
    let snapshot = inspected
        .pointer("/chart_metadata/0/self_contained_edit_snapshot")
        .expect("chart snapshot")
        .clone();
    let changed = json!({
        "type":"column",
        "title":"Changed",
        "categories":["Jan","Feb"],
        "series":[{"name":"Rate","values":[90,94]}],
        "show_legend":true
    });

    let stale_hash = presentation::replace_pptx_chart(
        &json!({
            "path":"chart-edit-source.pptx",
            "target_path":"chart-stale-hash.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":"0".repeat(64),
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":changed
        }),
        &state,
        &request,
    )
    .expect_err("stale chart hash must fail");
    assert!(stale_hash.to_string().contains("expected_chart_xml_sha256"));

    let mut wrong_snapshot = snapshot.clone();
    wrong_snapshot["title"] = json!("Stale title");
    let stale_snapshot = presentation::replace_pptx_chart(
        &json!({
            "path":"chart-edit-source.pptx",
            "target_path":"chart-stale-snapshot.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":hash,
            "expected_self_contained_edit_snapshot":wrong_snapshot,
            "replacement":changed
        }),
        &state,
        &request,
    )
    .expect_err("stale chart snapshot must fail");
    assert!(stale_snapshot
        .to_string()
        .contains("expected_self_contained_edit_snapshot"));

    let no_op = presentation::replace_pptx_chart(
        &json!({
            "path":"chart-edit-source.pptx",
            "target_path":"chart-no-op.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":hash,
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":snapshot
        }),
        &state,
        &request,
    )
    .expect_err("chart no-op must fail");
    assert!(no_op.to_string().contains("must change"));

    let in_place = presentation::replace_pptx_chart(
        &json!({
            "path":"chart-edit-source.pptx",
            "target_path":"chart-edit-source.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":hash,
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":changed,
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place chart replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    presentation::create_pptx(
        &json!({
            "target_path":"embedded-chart.pptx",
            "slides":[{"title":"Embedded","body":"Fixture"}]
        }),
        &state,
        &request,
    )
    .expect("create embedded chart fixture source");
    add_standard_pptx_chart_fixture(root.join("embedded-chart.pptx").as_path(), 1);
    let embedded_inspection =
        presentation::inspect_pptx_charts(&json!({"path":"embedded-chart.pptx"}), &state, &request)
            .expect("inspect embedded chart fixture");
    assert_eq!(
        embedded_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    let embedded_hash = embedded_inspection
        .pointer("/chart_metadata/0/chart_xml_sha256")
        .and_then(Value::as_str)
        .expect("embedded chart hash");
    let embedded = presentation::replace_pptx_chart(
        &json!({
            "path":"embedded-chart.pptx",
            "target_path":"embedded-chart-output.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":embedded_hash,
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":changed
        }),
        &state,
        &request,
    )
    .expect_err("embedded-workbook chart replacement must fail");
    assert!(embedded.to_string().contains("not eligible"));
    assert!(embedded.to_string().contains("relationships part"));

    fs::copy(source.as_path(), root.join("noncanonical-chart.pptx"))
        .expect("copy noncanonical chart fixture");
    rewrite_zip_text_entry(
        root.join("noncanonical-chart.pptx").as_path(),
        "ppt/charts/chart1.xml",
        |xml| xml.replacen("<c:style val=\"10\"/>", "<c:style val=\"11\"/>", 1),
    );
    let noncanonical_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"noncanonical-chart.pptx"}),
        &state,
        &request,
    )
    .expect("inspect noncanonical chart fixture");
    assert_eq!(
        noncanonical_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    assert!(noncanonical_inspection
        .pointer("/chart_metadata/0/self_contained_replacement_unsupported_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.contains("byte-exact canonical form")));
    let noncanonical_hash = noncanonical_inspection
        .pointer("/chart_metadata/0/chart_xml_sha256")
        .and_then(Value::as_str)
        .expect("noncanonical chart hash");
    let noncanonical = presentation::replace_pptx_chart(
        &json!({
            "path":"noncanonical-chart.pptx",
            "target_path":"noncanonical-chart-output.pptx",
            "slide_number":1,
            "chart_number":1,
            "expected_chart_xml_sha256":noncanonical_hash,
            "expected_self_contained_edit_snapshot":snapshot,
            "replacement":changed
        }),
        &state,
        &request,
    )
    .expect_err("noncanonical chart replacement must fail");
    assert!(noncanonical.to_string().contains("not eligible"));
    assert!(noncanonical
        .to_string()
        .contains("byte-exact canonical form"));

    fs::copy(source.as_path(), root.join("custom-axis-format-chart.pptx"))
        .expect("copy custom axis format fixture");
    rewrite_zip_text_entry(
        root.join("custom-axis-format-chart.pptx").as_path(),
        "ppt/charts/chart1.xml",
        |xml| {
            xml.replacen(
                "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>",
                "<c:numFmt formatCode=\"$#,##0\" sourceLinked=\"0\"/>",
                1,
            )
        },
    );
    let custom_format_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"custom-axis-format-chart.pptx"}),
        &state,
        &request,
    )
    .expect("inspect custom axis format chart fixture");
    assert_eq!(
        custom_format_inspection.pointer("/chart_metadata/0/value_axis_number_format"),
        Some(&json!("custom"))
    );
    assert_eq!(
        custom_format_inspection.pointer("/chart_metadata/0/value_axis_number_format_code"),
        Some(&json!("$#,##0"))
    );
    assert_eq!(
        custom_format_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    assert!(custom_format_inspection
        .pointer("/chart_metadata/0/self_contained_replacement_unsupported_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.contains("number format is unsupported")));

    fs::copy(source.as_path(), root.join("custom-axis-unit-chart.pptx"))
        .expect("copy custom axis unit fixture");
    rewrite_zip_text_entry(
        root.join("custom-axis-unit-chart.pptx").as_path(),
        "ppt/charts/chart1.xml",
        |xml| {
            xml.replacen(
                "<c:crossBetween val=\"between\"/>",
                "<c:crossBetween val=\"between\"/><c:majorUnit val=\"0\"/>",
                1,
            )
        },
    );
    let custom_unit_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"custom-axis-unit-chart.pptx"}),
        &state,
        &request,
    )
    .expect("inspect custom axis unit chart fixture");
    assert_eq!(
        custom_unit_inspection.pointer("/chart_metadata/0/value_axis_major_unit"),
        Some(&json!("0"))
    );
    assert_eq!(
        custom_unit_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    assert!(custom_unit_inspection
        .pointer("/chart_metadata/0/self_contained_replacement_unsupported_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.contains("units must be positive")));

    fs::copy(
        source.as_path(),
        root.join("custom-axis-log-base-chart.pptx"),
    )
    .expect("copy custom axis log-base fixture");
    rewrite_zip_text_entry(
        root.join("custom-axis-log-base-chart.pptx").as_path(),
        "ppt/charts/chart1.xml",
        |xml| {
            xml.replacen(
                "<c:valAx><c:axId val=\"45710656\"/><c:scaling><c:orientation",
                "<c:valAx><c:axId val=\"45710656\"/><c:scaling><c:logBase val=\"1\"/><c:orientation",
                1,
            )
        },
    );
    let custom_log_base_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"custom-axis-log-base-chart.pptx"}),
        &state,
        &request,
    )
    .expect("inspect custom axis log-base chart fixture");
    assert_eq!(
        custom_log_base_inspection.pointer("/chart_metadata/0/value_axis_log_base"),
        Some(&json!("1"))
    );
    assert_eq!(
        custom_log_base_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    assert!(custom_log_base_inspection
        .pointer("/chart_metadata/0/self_contained_replacement_unsupported_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.contains("log base must be between 2 and 1000")));

    fs::copy(
        source.as_path(),
        root.join("custom-axis-tick-mark-chart.pptx"),
    )
    .expect("copy custom axis tick-mark fixture");
    rewrite_zip_text_entry(
        root.join("custom-axis-tick-mark-chart.pptx").as_path(),
        "ppt/charts/chart1.xml",
        |xml| {
            xml.replacen(
                "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>",
                "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"sideways\"/>",
                1,
            )
        },
    );
    let custom_tick_mark_inspection = presentation::inspect_pptx_charts(
        &json!({"path":"custom-axis-tick-mark-chart.pptx"}),
        &state,
        &request,
    )
    .expect("inspect custom axis tick-mark chart fixture");
    assert_eq!(
        custom_tick_mark_inspection.pointer("/chart_metadata/0/value_axis_major_tick_mark"),
        Some(&json!("custom"))
    );
    assert_eq!(
        custom_tick_mark_inspection.pointer("/chart_metadata/0/value_axis_major_tick_mark_value"),
        Some(&json!("sideways"))
    );
    assert_eq!(
        custom_tick_mark_inspection
            .pointer("/chart_metadata/0/eligible_for_self_contained_chart_replacement"),
        Some(&json!(false))
    );
    assert!(custom_tick_mark_inspection
        .pointer("/chart_metadata/0/self_contained_replacement_unsupported_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.contains("major tick mark is unsupported")));

    assert_eq!(
        fs::read(source.as_path()).expect("chart source after rejected edits"),
        source_before
    );
    for output in [
        "chart-stale-hash.pptx",
        "chart-stale-snapshot.pptx",
        "chart-no-op.pptx",
        "embedded-chart-output.pptx",
        "noncanonical-chart-output.pptx",
    ] {
        assert!(!root.join(output).exists(), "unexpected output: {output}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_invalid_self_contained_pptx_chart_inputs_without_output() {
    let (root, state, request) = test_context();
    let cases = [
        (
            "missing.pptx",
            json!({"title":"Missing","layout":"chart"}),
            "require chart",
        ),
        (
            "wrong-layout.pptx",
            json!({
                "title":"Wrong",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"S","values":[1]}]}
            }),
            "only supported by the chart layout",
        ),
        (
            "mismatch.pptx",
            json!({
                "title":"Mismatch","layout":"chart",
                "chart":{"type":"column","categories":["A","B"],"series":[{"name":"S","values":[1]}]}
            }),
            "exactly one value per category",
        ),
        (
            "pie-series.pptx",
            json!({
                "title":"Pie","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S1","values":[1]},{"name":"S2","values":[2]}]}
            }),
            "exactly one series",
        ),
        (
            "pie-negative.pptx",
            json!({
                "title":"Pie","layout":"chart",
                "chart":{"type":"pie","categories":["A","B"],"series":[{"name":"S","values":[1,-1]}]}
            }),
            "non-negative",
        ),
        (
            "doughnut-series.pptx",
            json!({
                "title":"Doughnut","layout":"chart",
                "chart":{"type":"doughnut","categories":["A"],"series":[{"name":"S1","values":[1]},{"name":"S2","values":[2]}]}
            }),
            "exactly one series",
        ),
        (
            "doughnut-negative.pptx",
            json!({
                "title":"Doughnut","layout":"chart",
                "chart":{"type":"doughnut","categories":["A","B"],"series":[{"name":"S","values":[1,-1]}]}
            }),
            "non-negative",
        ),
        (
            "doughnut-zero.pptx",
            json!({
                "title":"Doughnut","layout":"chart",
                "chart":{"type":"doughnut","categories":["A","B"],"series":[{"name":"S","values":[0,0]}]}
            }),
            "at least one positive value",
        ),
        (
            "area-percentage-labels.pptx",
            json!({
                "title":"Area","layout":"chart",
                "chart":{"type":"area","categories":["A"],"series":[{"name":"S","values":[1]}],"data_labels":"percentage"}
            }),
            "only for pie or doughnut",
        ),
        (
            "pie-axis-title.pptx",
            json!({
                "title":"Pie","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1]}],"category_axis_title":"Category"}
            }),
            "does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
        ),
        (
            "pie-secondary-axis.pptx",
            json!({
                "title":"Pie secondary","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1],"value_axis":"secondary"}]}
            }),
            "must use the primary value_axis",
        ),
        (
            "all-secondary-series.pptx",
            json!({
                "title":"All secondary","layout":"chart",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"S1","values":[1],"value_axis":"secondary"},{"name":"S2","values":[2],"value_axis":"secondary"}]}
            }),
            "requires at least one primary series",
        ),
        (
            "secondary-title-without-series.pptx",
            json!({
                "title":"Secondary title","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"secondary_value_axis_title":"Secondary"}
            }),
            "requires at least one secondary series",
        ),
        (
            "pie-axis-number-format.pptx",
            json!({
                "title":"Pie format","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_number_format":"integer"}
            }),
            "does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
        ),
        (
            "pie-axis-unit.pptx",
            json!({
                "title":"Pie unit","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_major_unit":1}
            }),
            "does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
        ),
        (
            "pie-log-axis.pptx",
            json!({
                "title":"Pie logarithmic","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_log_base":10}
            }),
            "does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
        ),
        (
            "pie-axis-tick-mark.pptx",
            json!({
                "title":"Pie ticks","layout":"chart",
                "chart":{"type":"pie","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_major_tick_mark":"outside"}
            }),
            "does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
        ),
        (
            "primary-minimum-hides-values.pptx",
            json!({
                "title":"Hidden minimum","layout":"chart",
                "chart":{"type":"column","categories":["A","B"],"series":[{"name":"S","values":[1,2]}],"value_axis_minimum":1.5}
            }),
            "primary value-axis minimum would hide series values",
        ),
        (
            "primary-maximum-hides-values.pptx",
            json!({
                "title":"Hidden maximum","layout":"chart",
                "chart":{"type":"line","categories":["A","B"],"series":[{"name":"S","values":[1,2]}],"value_axis_maximum":1.5}
            }),
            "primary value-axis maximum would hide series values",
        ),
        (
            "invalid-primary-range.pptx",
            json!({
                "title":"Invalid range","layout":"chart",
                "chart":{"type":"area","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_minimum":2,"value_axis_maximum":2}
            }),
            "primary value-axis minimum must be below its maximum",
        ),
        (
            "secondary-range-without-series.pptx",
            json!({
                "title":"Missing secondary","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"secondary_value_axis_maximum":2}
            }),
            "secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series",
        ),
        (
            "secondary-unit-without-series.pptx",
            json!({
                "title":"Missing secondary unit","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"secondary_value_axis_major_unit":1}
            }),
            "secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series",
        ),
        (
            "secondary-log-without-series.pptx",
            json!({
                "title":"Missing secondary logarithmic axis","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"secondary_value_axis_log_base":10}
            }),
            "secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series",
        ),
        (
            "secondary-tick-mark-without-series.pptx",
            json!({
                "title":"Missing secondary ticks","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"secondary_value_axis_minor_tick_mark":"inside"}
            }),
            "secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series",
        ),
        (
            "secondary-minimum-hides-values.pptx",
            json!({
                "title":"Hidden secondary","layout":"chart",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"Primary","values":[1]},{"name":"Secondary","values":[10],"value_axis":"secondary"}],"secondary_value_axis_minimum":11}
            }),
            "secondary value-axis minimum would hide series values",
        ),
        (
            "unknown-axis-number-format.pptx",
            json!({
                "title":"Unknown format","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_number_format":"accounting"}
            }),
            "unsupported PPTX chart value-axis number format",
        ),
        (
            "unknown-axis-tick-mark.pptx",
            json!({
                "title":"Unknown ticks","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_major_tick_mark":"diagonal"}
            }),
            "unsupported PPTX chart value-axis tick mark",
        ),
        (
            "zero-major-unit.pptx",
            json!({
                "title":"Zero unit","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_major_unit":0}
            }),
            "value_axis_major_unit must be positive",
        ),
        (
            "log-base-below-range.pptx",
            json!({
                "title":"Invalid logarithmic base","layout":"chart",
                "chart":{"type":"column","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_log_base":1}
            }),
            "value_axis_log_base must be between 2 and 1000",
        ),
        (
            "log-axis-zero-value.pptx",
            json!({
                "title":"Zero logarithmic value","layout":"chart",
                "chart":{"type":"line","categories":["A","B"],"series":[{"name":"S","values":[1,0]}],"value_axis_log_base":10}
            }),
            "primary logarithmic value axis requires every series value to be positive",
        ),
        (
            "log-axis-zero-bound.pptx",
            json!({
                "title":"Zero logarithmic bound","layout":"chart",
                "chart":{"type":"area","categories":["A"],"series":[{"name":"S","values":[5]}],"value_axis_minimum":0,"value_axis_log_base":10}
            }),
            "primary logarithmic value-axis bounds must be positive",
        ),
        (
            "minor-not-below-major.pptx",
            json!({
                "title":"Invalid units","layout":"chart",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"S","values":[1]}],"value_axis_major_unit":2,"value_axis_minor_unit":2}
            }),
            "primary value-axis minor unit must be below its major unit",
        ),
        (
            "major-unit-exceeds-range.pptx",
            json!({
                "title":"Oversized unit","layout":"chart",
                "chart":{"type":"area","categories":["A"],"series":[{"name":"S","values":[5]}],"value_axis_minimum":0,"value_axis_maximum":10,"value_axis_major_unit":20}
            }),
            "primary value-axis major unit exceeds its explicit range",
        ),
        (
            "unknown-series-axis.pptx",
            json!({
                "title":"Unknown axis","layout":"chart",
                "chart":{"type":"area","categories":["A"],"series":[{"name":"S","values":[1],"value_axis":"tertiary"}]}
            }),
            "unsupported PPTX chart value_axis",
        ),
        (
            "hidden-left-legend.pptx",
            json!({
                "title":"Hidden legend","layout":"chart",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"S","values":[1]}],"show_legend":false,"legend_position":"left"}
            }),
            "must be right when show_legend=false",
        ),
        (
            "duplicate-series.pptx",
            json!({
                "title":"Duplicate","layout":"chart",
                "chart":{"type":"line","categories":["A"],"series":[{"name":"S","values":[1]},{"name":"S","values":[2]}]}
            }),
            "names must be unique",
        ),
    ];
    for (target, slide, expected) in cases {
        let error = presentation::create_pptx(
            &json!({"target_path":target,"slides":[slide]}),
            &state,
            &request,
        )
        .expect_err("invalid chart input must fail closed");
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!root.join(target).exists(), "unexpected output: {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn appends_simple_pptx_table_layout_without_modifying_source() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"Original","body":"Preserved"}]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");
    let appended = presentation::append_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"appended-table.pptx",
            "slides":[{
                "title":"Inventory",
                "layout":"table",
                "table":{
                    "header_row":false,
                    "cells":[["Widget","12"],["Gadget","8"]]
                }
            }]
        }),
        &state,
        &request,
    )
    .expect("append table-layout slide");
    assert_eq!(
        appended.get("appended_slides").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after append"),
        source_before
    );
    let table = presentation::inspect_pptx_table(
        &json!({"path":"appended-table.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect appended table");
    assert_eq!(
        table.pointer("/cell_text/1/0").and_then(Value::as_str),
        Some("Gadget")
    );
    assert_eq!(
        table
            .get("eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut archive = ZipArchive::new(
        File::open(root.join("appended-table.pptx")).expect("appended table PPTX file"),
    )
    .expect("appended table PPTX ZIP");
    let slide =
        read_zip_text(&mut archive, "ppt/slides/slide2.xml").expect("appended table slide XML");
    assert!(slide.contains("<a:tblPr firstRow=\"0\" bandRow=\"1\">"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_invalid_pptx_table_layout_inputs_without_output() {
    let (root, state, request) = test_context();
    let missing = presentation::create_pptx(
        &json!({"target_path":"missing.pptx","slides":[{"title":"Missing","layout":"table"}]}),
        &state,
        &request,
    )
    .expect_err("table layout without table must fail");
    assert!(missing.to_string().contains("require table"));

    let unrelated = presentation::create_pptx(
        &json!({
            "target_path":"unrelated.pptx",
            "slides":[{"title":"Invalid","layout":"table","body":"Not allowed","table":{"cells":[["A"]]}}]
        }),
        &state,
        &request,
    )
    .expect_err("table layout with body must fail");
    assert!(unrelated.to_string().contains("do not support body"));

    let wrong_layout = presentation::create_pptx(
        &json!({
            "target_path":"wrong-layout.pptx",
            "slides":[{"title":"Invalid","table":{"cells":[["A"]]}}]
        }),
        &state,
        &request,
    )
    .expect_err("table on non-table layout must fail");
    assert!(wrong_layout
        .to_string()
        .contains("only supported by the table layout"));

    let ragged = presentation::create_pptx(
        &json!({
            "target_path":"ragged.pptx",
            "slides":[{"title":"Invalid","layout":"table","table":{"cells":[["A","B"],["C"]]}}]
        }),
        &state,
        &request,
    )
    .expect_err("ragged table must fail");
    assert!(ragged.to_string().contains("rectangular matrix"));

    let too_many_rows = vec![vec!["x"; 1]; 51];
    let oversized = presentation::create_pptx(
        &json!({
            "target_path":"oversized.pptx",
            "slides":[{"title":"Invalid","layout":"table","table":{"cells":too_many_rows}}]
        }),
        &state,
        &request,
    )
    .expect_err("oversized table must fail");
    assert!(oversized.to_string().contains("between 1 and 50 rows"));

    let large_cell = "x".repeat(10_000);
    let excessive_text = vec![vec![large_cell; 11]];
    let excessive = presentation::create_pptx(
        &json!({
            "target_path":"excessive-text.pptx",
            "slides":[{"title":"Invalid","layout":"table","table":{"cells":excessive_text}}]
        }),
        &state,
        &request,
    )
    .expect_err("excessive table text must fail");
    assert!(excessive
        .to_string()
        .contains("100000 character safety limit"));

    for path in [
        "missing.pptx",
        "unrelated.pptx",
        "wrong-layout.pptx",
        "ragged.pptx",
        "oversized.pptx",
        "excessive-text.pptx",
    ] {
        assert!(!root.join(path).exists(), "unexpected output: {path}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn appends_pptx_slides_without_modifying_source_or_existing_parts() {
    let (root, state, request) = test_context();
    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::create_dir_all(root.join("assets")).expect("assets");
    fs::write(root.join("assets/pixel.png"), image).expect("write PNG");
    presentation::create_pptx(
        &json!({
            "target_path":"artifacts/source.pptx",
            "slides":[{
                "title":"Original",
                "body":"This slide and the inherited package parts must remain unchanged.",
                "notes":"Existing notes create a reusable notes master."
            }]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_path = root.join("artifacts/source.pptx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source ZIP");
    let original_slide =
        read_zip_text(&mut source_archive, "ppt/slides/slide1.xml").expect("original slide");
    let original_theme =
        read_zip_text(&mut source_archive, "ppt/theme/theme1.xml").expect("original theme");
    drop(source_archive);

    let updated = presentation::append_pptx_slides(
        &json!({
            "path":"artifacts/source.pptx",
            "target_path":"artifacts/appended.pptx",
            "slides":[
                {
                    "title":"Appended image",
                    "layout":"image_right",
                    "body":"- Existing theme retained\n- New image is package-local",
                    "image":{"path":"assets/pixel.png","alt_text":"One pixel test image","fit":"contain"}
                },
                {
                    "title":"Appended notes",
                    "body":"A second appended slide.",
                    "notes":"These notes reuse the existing notes master."
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("append PPTX slides");
    assert_eq!(
        updated.get("previous_slides").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        updated.get("appended_slides").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        updated.get("appended_images").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after append"),
        source_before
    );

    let inspected =
        presentation::inspect_pptx(&json!({"path":"artifacts/appended.pptx"}), &state, &request)
            .expect("inspect appended PPTX");
    assert_eq!(inspected.get("slides").and_then(Value::as_u64), Some(3));
    assert_eq!(inspected.get("images").and_then(Value::as_u64), Some(1));
    assert_eq!(
        inspected.get("speaker_notes").and_then(Value::as_u64),
        Some(2)
    );
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata[2].get("title").and_then(Value::as_str),
        Some("Appended notes")
    );

    let mut appended_archive =
        ZipArchive::new(File::open(root.join("artifacts/appended.pptx")).expect("appended file"))
            .expect("appended ZIP");
    assert_eq!(
        read_zip_text(&mut appended_archive, "ppt/slides/slide1.xml").expect("preserved slide"),
        original_slide
    );
    assert_eq!(
        read_zip_text(&mut appended_archive, "ppt/theme/theme1.xml").expect("preserved theme"),
        original_theme
    );
    assert!(appended_archive
        .by_name("ppt/media/chatosImage1.png")
        .is_ok());
    assert!(appended_archive
        .by_name("ppt/notesSlides/notesSlide2.xml")
        .is_ok());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_append_rejects_in_place_editing_and_notes_without_existing_master() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"Original","body":"No notes master is present."}]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");
    let in_place = presentation::append_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"source.pptx",
            "slides":[{"title":"Rejected"}]
        }),
        &state,
        &request,
    )
    .expect_err("in-place edit must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let missing_master = presentation::append_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"with-notes.pptx",
            "slides":[{"title":"Notes","notes":"No notes master exists."}]
        }),
        &state,
        &request,
    )
    .expect_err("missing notes master must fail");
    assert!(missing_master
        .to_string()
        .contains("existing internal notes master"));
    assert!(!root.join("with-notes.pptx").exists());
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deletes_pptx_slides_by_visible_order_and_removes_owned_notes_parts() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"base.pptx",
            "slides":[
                {"title":"First","body":"Alpha"},
                {"title":"Second","body":"Beta","notes":"Delete these notes"},
                {"title":"Third","body":"Gamma"},
                {"title":"Fourth","body":"Delta"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create base PPTX");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"base.pptx",
            "target_path":"source.pptx",
            "slide_order":[3,1,4,2]
        }),
        &state,
        &request,
    )
    .expect("reorder source PPTX");
    let source_path = root.join("source.pptx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source ZIP");
    let first_slide_before =
        read_zip_text(&mut source_archive, "ppt/slides/slide1.xml").expect("first slide");
    let fourth_slide_before =
        read_zip_text(&mut source_archive, "ppt/slides/slide4.xml").expect("fourth slide");
    drop(source_archive);

    let updated = presentation::delete_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"deleted.pptx",
            "slide_numbers":[1,4]
        }),
        &state,
        &request,
    )
    .expect("delete PPTX slides");
    assert_eq!(updated.get("deleted_slides"), Some(&json!([1, 4])));
    assert_eq!(
        updated.get("deleted_slide_files"),
        Some(&json!(["ppt/slides/slide3.xml", "ppt/slides/slide2.xml"]))
    );
    assert_eq!(
        updated.get("deleted_speaker_notes").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(updated.get("slides").and_then(Value::as_u64), Some(2));
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after deletion"),
        source_before
    );

    let inspected = presentation::inspect_pptx(&json!({"path":"deleted.pptx"}), &state, &request)
        .expect("inspect deleted PPTX");
    assert_eq!(
        inspected.get("slide_files"),
        Some(&json!(["ppt/slides/slide1.xml", "ppt/slides/slide4.xml"]))
    );
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata
            .iter()
            .map(|slide| slide
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["First", "Fourth"]
    );
    assert_eq!(
        inspected.get("speaker_notes").and_then(Value::as_u64),
        Some(0)
    );

    let mut deleted_archive =
        ZipArchive::new(File::open(root.join("deleted.pptx")).expect("deleted file"))
            .expect("deleted ZIP");
    assert_eq!(
        read_zip_text(&mut deleted_archive, "ppt/slides/slide1.xml")
            .expect("preserved first slide"),
        first_slide_before
    );
    assert_eq!(
        read_zip_text(&mut deleted_archive, "ppt/slides/slide4.xml")
            .expect("preserved fourth slide"),
        fourth_slide_before
    );
    for removed in [
        "ppt/slides/slide2.xml",
        "ppt/slides/_rels/slide2.xml.rels",
        "ppt/slides/slide3.xml",
        "ppt/slides/_rels/slide3.xml.rels",
        "ppt/notesSlides/notesSlide1.xml",
        "ppt/notesSlides/_rels/notesSlide1.xml.rels",
    ] {
        assert!(
            deleted_archive.by_name(removed).is_err(),
            "{removed} remains"
        );
    }
    let presentation_relationships =
        read_zip_text(&mut deleted_archive, "ppt/_rels/presentation.xml.rels")
            .expect("presentation relationships");
    assert!(!presentation_relationships.contains("slides/slide2.xml"));
    assert!(!presentation_relationships.contains("slides/slide3.xml"));
    let content_types =
        read_zip_text(&mut deleted_archive, "[Content_Types].xml").expect("content types");
    assert!(!content_types.contains("/ppt/slides/slide2.xml"));
    assert!(!content_types.contains("/ppt/slides/slide3.xml"));
    assert!(!content_types.contains("/ppt/notesSlides/notesSlide1.xml"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_slide_deletion_rejects_empty_duplicate_range_all_and_in_place_requests() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"One"},{"title":"Two"},{"title":"Three"}]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");
    for (target, slides, expected) in [
        ("empty.pptx", json!([]), "leaving at least one"),
        ("all.pptx", json!([1, 2, 3]), "leaving at least one"),
        (
            "duplicate.pptx",
            json!([1, 1]),
            "must not contain duplicates",
        ),
        ("range.pptx", json!([4]), "out-of-range"),
    ] {
        let error = presentation::delete_pptx_slides(
            &json!({
                "path":"source.pptx",
                "target_path":target,
                "slide_numbers":slides
            }),
            &state,
            &request,
        )
        .expect_err("invalid slide deletion must fail");
        assert!(error.to_string().contains(expected));
        assert!(!root.join(target).exists());
    }
    let in_place = presentation::delete_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"source.pptx",
            "slide_numbers":[2]
        }),
        &state,
        &request,
    )
    .expect_err("in-place deletion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reorders_pptx_slides_by_visible_presentation_order_without_modifying_parts() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[
                {"title":"First","body":"Alpha"},
                {"title":"Second","body":"Beta","notes":"Keep these notes"},
                {"title":"Third","body":"Gamma"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_path = root.join("source.pptx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source ZIP");
    let source_relationships =
        read_zip_text(&mut source_archive, "ppt/_rels/presentation.xml.rels")
            .expect("presentation relationships");
    let slide_parts_before = (1..=3)
        .map(|number| {
            read_zip_text(
                &mut source_archive,
                format!("ppt/slides/slide{number}.xml").as_str(),
            )
            .expect("slide XML")
        })
        .collect::<Vec<_>>();
    let notes_before =
        read_zip_text(&mut source_archive, "ppt/notesSlides/notesSlide1.xml").expect("notes XML");
    drop(source_archive);

    let updated = presentation::reorder_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"reordered.pptx",
            "slide_order":[3,1,2]
        }),
        &state,
        &request,
    )
    .expect("reorder PPTX slides");
    assert_eq!(updated.get("slides").and_then(Value::as_u64), Some(3));
    assert_eq!(updated.get("slide_order"), Some(&json!([3, 1, 2])));
    assert_eq!(
        updated.get("slide_files"),
        Some(&json!([
            "ppt/slides/slide3.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml"
        ]))
    );
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after reorder"),
        source_before
    );

    let inspected = presentation::inspect_pptx(&json!({"path":"reordered.pptx"}), &state, &request)
        .expect("inspect reordered PPTX");
    assert_eq!(
        inspected.get("slide_files"),
        Some(&json!([
            "ppt/slides/slide3.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml"
        ]))
    );
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata
            .iter()
            .map(|slide| slide
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["Third", "First", "Second"]
    );
    assert_eq!(metadata[2].get("notes_present"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata[2].get("notes_preview").and_then(Value::as_str),
        Some("Keep these notes")
    );

    let mut reordered_archive =
        ZipArchive::new(File::open(root.join("reordered.pptx")).expect("reordered file"))
            .expect("reordered ZIP");
    assert_eq!(
        read_zip_text(&mut reordered_archive, "ppt/_rels/presentation.xml.rels")
            .expect("reordered presentation relationships"),
        source_relationships
    );
    for (index, expected) in slide_parts_before.iter().enumerate() {
        assert_eq!(
            read_zip_text(
                &mut reordered_archive,
                format!("ppt/slides/slide{}.xml", index + 1).as_str(),
            )
            .expect("preserved slide XML"),
            *expected
        );
    }
    assert_eq!(
        read_zip_text(&mut reordered_archive, "ppt/notesSlides/notesSlide1.xml")
            .expect("preserved notes XML"),
        notes_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_reordering_requires_a_changed_full_permutation_and_distinct_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"One"},{"title":"Two"},{"title":"Three"}]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");
    for (target, order, expected) in [
        ("identity.pptx", json!([1, 2, 3]), "must change"),
        ("missing.pptx", json!([3, 1]), "every current slide"),
        (
            "duplicate.pptx",
            json!([3, 1, 1]),
            "must not contain duplicates",
        ),
        ("range.pptx", json!([3, 1, 4]), "out-of-range"),
    ] {
        let error = presentation::reorder_pptx_slides(
            &json!({
                "path":"source.pptx",
                "target_path":target,
                "slide_order":order
            }),
            &state,
            &request,
        )
        .expect_err("invalid slide order must fail");
        assert!(error.to_string().contains(expected));
        assert!(!root.join(target).exists());
    }
    let in_place = presentation::reorder_pptx_slides(
        &json!({
            "path":"source.pptx",
            "target_path":"source.pptx",
            "slide_order":[3,1,2]
        }),
        &state,
        &request,
    )
    .expect_err("in-place reorder must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_exact_pptx_text_runs_on_selected_slides_without_modifying_source() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[
                {"title":"Quarterly Review","body":"Quarterly result"},
                {"title":"Quarterly Review","body":"Quarterly Quarterly Quarterly"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_path = root.join("source.pptx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source ZIP");
    let slide_one_before =
        read_zip_text(&mut source_archive, "ppt/slides/slide1.xml").expect("slide one");
    let slide_two_before =
        read_zip_text(&mut source_archive, "ppt/slides/slide2.xml").expect("slide two");
    let run_properties_before = slide_two_before.matches("<a:rPr").count();
    drop(source_archive);

    let updated = presentation::replace_pptx_text(
        &json!({
            "path":"source.pptx",
            "target_path":"replaced.pptx",
            "find":"Quarterly",
            "replacement":"Annual",
            "slide_numbers":[2],
            "max_replacements":2
        }),
        &state,
        &request,
    )
    .expect("replace PPTX text");
    assert_eq!(updated.get("replacements").and_then(Value::as_u64), Some(2));
    assert_eq!(
        updated
            .get("replacement_limit_reached")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(updated.get("matched_slides"), Some(&json!([2])));
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after replace"),
        source_before
    );

    let inspected = presentation::inspect_pptx(&json!({"path":"replaced.pptx"}), &state, &request)
        .expect("inspect replaced PPTX");
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata[0].get("title").and_then(Value::as_str),
        Some("Quarterly Review")
    );
    assert_eq!(
        metadata[1].get("title").and_then(Value::as_str),
        Some("Annual Review")
    );
    assert!(metadata[1]
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Annual Quarterly Quarterly")));

    let mut replaced_archive =
        ZipArchive::new(File::open(root.join("replaced.pptx")).expect("replaced file"))
            .expect("replaced ZIP");
    assert_eq!(
        read_zip_text(&mut replaced_archive, "ppt/slides/slide1.xml").expect("preserved slide one"),
        slide_one_before
    );
    let slide_two_after =
        read_zip_text(&mut replaced_archive, "ppt/slides/slide2.xml").expect("slide two after");
    assert_eq!(
        slide_two_after.matches("<a:rPr").count(),
        run_properties_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_text_replacement_rejects_cross_run_matches_and_invalid_selection() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"Alpha","body":"Beta"}]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");
    let cross_run = presentation::replace_pptx_text(
        &json!({
            "path":"source.pptx",
            "target_path":"cross-run.pptx",
            "find":"AlphaBeta",
            "replacement":"Combined"
        }),
        &state,
        &request,
    )
    .expect_err("cross-run replacement must fail");
    assert!(cross_run
        .to_string()
        .contains("single visible DrawingML text run"));
    assert!(!root.join("cross-run.pptx").exists());

    let invalid_slide = presentation::replace_pptx_text(
        &json!({
            "path":"source.pptx",
            "target_path":"invalid-slide.pptx",
            "find":"Alpha",
            "replacement":"Gamma",
            "slide_numbers":[2]
        }),
        &state,
        &request,
    )
    .expect_err("out-of-range slide must fail");
    assert!(invalid_slide.to_string().contains("out-of-range"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_unique_same_format_pptx_text_across_runs_in_visible_slide_order() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"fixture.pptx",
            "slides":[
                {"title":"Target","body":"Quarterly Review"},
                {"title":"Other","body":"Untouched"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create cross-run fixture");
    split_pptx_text_run(
        root.join("fixture.pptx").as_path(),
        "ppt/slides/slide1.xml",
        "Quarterly Review",
        &["Quarter", "ly Rev", "iew"],
    );
    presentation::reorder_pptx_slides(
        &json!({
            "path":"fixture.pptx",
            "target_path":"source.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder cross-run fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");

    let updated = presentation::replace_pptx_text_across_runs(
        &json!({
            "path":"source.pptx",
            "target_path":"replaced.pptx",
            "selection":"Quarterly Review",
            "replacement":"Annual Summary",
            "slide_numbers":[2]
        }),
        &state,
        &request,
    )
    .expect("replace PPTX text across runs");
    assert_eq!(
        updated.get("matched_slide").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(updated.get("runs_touched").and_then(Value::as_u64), Some(3));
    assert_eq!(updated.get("emptied_runs").and_then(Value::as_u64), Some(2));
    assert_eq!(
        updated
            .get("globally_unique_match")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after replacement"),
        source_before
    );

    let inspected = presentation::inspect_pptx(&json!({"path":"replaced.pptx"}), &state, &request)
        .expect("inspect cross-run replacement");
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata[0].get("title").and_then(Value::as_str),
        Some("Other")
    );
    assert!(metadata[1]
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Annual Summary")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspects_and_replaces_simple_pptx_table_cell_by_visible_slide_order() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"fixture.pptx",
            "slides":[
                {"title":"Table slide","body":"Editable table below"},
                {"title":"Other slide","body":"Untouched"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create table fixture");
    insert_simple_pptx_table(root.join("fixture.pptx").as_path(), "ppt/slides/slide1.xml");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"fixture.pptx",
            "target_path":"source.pptx",
            "slide_order":[2,1]
        }),
        &state,
        &request,
    )
    .expect("reorder table fixture");
    let source_before = fs::read(root.join("source.pptx")).expect("table source bytes");

    let deck = presentation::inspect_pptx(&json!({"path":"source.pptx"}), &state, &request)
        .expect("inspect table deck");
    assert_eq!(deck.get("tables").and_then(Value::as_u64), Some(1));
    assert_eq!(
        deck.pointer("/slide_metadata/1/tables")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        deck.pointer("/slide_metadata/1/table_metadata/0/eligible_for_cell_replacement")
            .and_then(Value::as_bool),
        Some(true)
    );

    let table = presentation::inspect_pptx_table(
        &json!({"path":"source.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect simple PPTX table");
    assert_eq!(table.get("rows").and_then(Value::as_u64), Some(2));
    assert_eq!(table.get("columns").and_then(Value::as_u64), Some(2));
    assert_eq!(
        table.pointer("/cell_text/1/1").and_then(Value::as_str),
        Some("120")
    );

    let updated = presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"source.pptx",
            "target_path":"replaced.pptx",
            "slide_number":2,
            "table_number":1,
            "row":2,
            "column":2,
            "expected_text":"120",
            "replacement":"145"
        }),
        &state,
        &request,
    )
    .expect("replace simple PPTX table cell");
    assert_eq!(updated.get("row").and_then(Value::as_u64), Some(2));
    assert_eq!(updated.get("column").and_then(Value::as_u64), Some(2));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after table replacement"),
        source_before
    );
    let replaced = presentation::inspect_pptx_table(
        &json!({"path":"replaced.pptx","slide_number":2,"table_number":1}),
        &state,
        &request,
    )
    .expect("inspect replaced PPTX table");
    assert_eq!(
        replaced.pointer("/cell_text/1/1").and_then(Value::as_str),
        Some("145")
    );
    assert_eq!(
        replaced.pointer("/cell_text/0/1").and_then(Value::as_str),
        Some("Revenue")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_unsafe_or_stale_pptx_table_cell_replacement_without_output() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[{"title":"Table slide","body":"Editable table below"}]
        }),
        &state,
        &request,
    )
    .expect("create unsafe table fixture");
    insert_simple_pptx_table(root.join("source.pptx").as_path(), "ppt/slides/slide1.xml");
    let source_before = fs::read(root.join("source.pptx")).expect("unsafe table source bytes");

    let stale = presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"source.pptx","target_path":"stale.pptx","slide_number":1,
            "table_number":1,"row":2,"column":2,"expected_text":"119","replacement":"145"
        }),
        &state,
        &request,
    )
    .expect_err("stale table snapshot must fail");
    assert!(stale.to_string().contains("does not match expected_text"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after stale snapshot rejection"),
        source_before
    );

    rewrite_zip_text_entry(
        root.join("source.pptx").as_path(),
        "ppt/slides/slide1.xml",
        |xml| xml.replacen("<a:tc><a:txBody>", "<a:tc gridSpan=\"2\"><a:txBody>", 1),
    );
    let merged_source_before =
        fs::read(root.join("source.pptx")).expect("merged table source bytes");
    let merged = presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"source.pptx","target_path":"merged.pptx","slide_number":1,
            "table_number":1,"row":1,"column":1,"expected_text":"Region","replacement":"Area"
        }),
        &state,
        &request,
    )
    .expect_err("merged table cell must fail");
    assert!(merged.to_string().contains("merged or attributed"));
    let in_place = presentation::replace_pptx_table_cell_text(
        &json!({
            "path":"source.pptx","target_path":"source.pptx","slide_number":1,
            "table_number":1,"row":1,"column":1,"expected_text":"Region","replacement":"Area",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place table edit must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert!(!root.join("stale.pptx").exists());
    assert!(!root.join("merged.pptx").exists());
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after unsafe rejections"),
        merged_source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_exact_pptx_notes_runs_by_visible_order_without_modifying_slides_or_source() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"base.pptx",
            "slides":[
                {"title":"First","body":"Visible Quarterly one","notes":"First notes"},
                {"title":"Second","body":"Visible Quarterly two","notes":"Second notes"},
                {"title":"Quarterly Third","body":"Visible Quarterly three","notes":"Quarterly Quarterly Quarterly"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    presentation::reorder_pptx_slides(
        &json!({
            "path":"base.pptx",
            "target_path":"source.pptx",
            "slide_order":[3,1,2]
        }),
        &state,
        &request,
    )
    .expect("reorder source PPTX");

    let source_path = root.join("source.pptx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source ZIP");
    let slide_parts_before = (1..=3)
        .map(|number| {
            read_zip_text(
                &mut source_archive,
                format!("ppt/slides/slide{number}.xml").as_str(),
            )
            .expect("slide XML")
        })
        .collect::<Vec<_>>();
    let notes_parts_before = (1..=3)
        .map(|number| {
            read_zip_text(
                &mut source_archive,
                format!("ppt/notesSlides/notesSlide{number}.xml").as_str(),
            )
            .expect("notes XML")
        })
        .collect::<Vec<_>>();
    let third_notes_run_properties = notes_parts_before[2].matches("<a:rPr").count();
    drop(source_archive);

    let updated = presentation::replace_pptx_notes_text(
        &json!({
            "path":"source.pptx",
            "target_path":"notes-replaced.pptx",
            "find":"Quarterly",
            "replacement":"Annual",
            "slide_numbers":[1],
            "max_replacements":2
        }),
        &state,
        &request,
    )
    .expect("replace PPTX speaker-note text");
    assert_eq!(updated.get("matched_slides"), Some(&json!([1])));
    assert_eq!(updated.get("replacements").and_then(Value::as_u64), Some(2));
    assert_eq!(
        updated
            .get("replacement_limit_reached")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after notes replacement"),
        source_before
    );

    let inspected =
        presentation::inspect_pptx(&json!({"path":"notes-replaced.pptx"}), &state, &request)
            .expect("inspect notes-replaced PPTX");
    let metadata = inspected
        .get("slide_metadata")
        .and_then(Value::as_array)
        .expect("slide metadata");
    assert_eq!(
        metadata[0].get("title").and_then(Value::as_str),
        Some("Quarterly Third")
    );
    assert_eq!(
        metadata[0].get("notes_preview").and_then(Value::as_str),
        Some("Annual Annual Quarterly")
    );
    assert_eq!(
        metadata[1].get("notes_preview").and_then(Value::as_str),
        Some("First notes")
    );
    assert_eq!(
        metadata[2].get("notes_preview").and_then(Value::as_str),
        Some("Second notes")
    );

    let mut output_archive =
        ZipArchive::new(File::open(root.join("notes-replaced.pptx")).expect("output file"))
            .expect("output ZIP");
    for (index, expected) in slide_parts_before.iter().enumerate() {
        assert_eq!(
            read_zip_text(
                &mut output_archive,
                format!("ppt/slides/slide{}.xml", index + 1).as_str(),
            )
            .expect("preserved slide XML"),
            *expected
        );
    }
    for (index, expected) in notes_parts_before.iter().take(2).enumerate() {
        assert_eq!(
            read_zip_text(
                &mut output_archive,
                format!("ppt/notesSlides/notesSlide{}.xml", index + 1).as_str(),
            )
            .expect("preserved unselected notes XML"),
            *expected
        );
    }
    let third_notes_after = read_zip_text(&mut output_archive, "ppt/notesSlides/notesSlide3.xml")
        .expect("updated notes XML");
    assert_eq!(
        third_notes_after.matches("<a:rPr").count(),
        third_notes_run_properties
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_notes_replacement_rejects_cross_run_missing_notes_invalid_selection_and_in_place() {
    let (root, state, request) = test_context();
    presentation::create_pptx(
        &json!({
            "target_path":"source.pptx",
            "slides":[
                {"title":"With notes","notes":"Alpha\nBeta"},
                {"title":"Without notes"}
            ]
        }),
        &state,
        &request,
    )
    .expect("create source PPTX");
    let source_before = fs::read(root.join("source.pptx")).expect("source bytes");

    for (target, find, slides, expected) in [
        (
            "cross-run-notes.pptx",
            "AlphaBeta",
            json!([1]),
            "single DrawingML text run",
        ),
        (
            "missing-notes.pptx",
            "Alpha",
            json!([2]),
            "single DrawingML text run",
        ),
        (
            "invalid-notes-slide.pptx",
            "Alpha",
            json!([3]),
            "out-of-range",
        ),
    ] {
        let error = presentation::replace_pptx_notes_text(
            &json!({
                "path":"source.pptx",
                "target_path":target,
                "find":find,
                "replacement":"Combined",
                "slide_numbers":slides
            }),
            &state,
            &request,
        )
        .expect_err("invalid speaker-note replacement must fail");
        assert!(error.to_string().contains(expected));
        assert!(!root.join(target).exists());
    }

    let in_place = presentation::replace_pptx_notes_text(
        &json!({
            "path":"source.pptx",
            "target_path":"source.pptx",
            "find":"Alpha",
            "replacement":"Gamma"
        }),
        &state,
        &request,
    )
    .expect_err("in-place speaker-note replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("source.pptx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pptx_creation_rejects_missing_or_invalid_images_and_unsafe_text() {
    let (root, state, request) = test_context();
    fs::write(root.join("fake.png"), b"not a png").expect("fake image");
    let missing = presentation::create_pptx(
        &json!({
            "target_path":"missing.pptx",
            "slides":[{"title":"Image","layout":"image_right","body":"No image"}]
        }),
        &state,
        &request,
    )
    .expect_err("missing image must fail");
    assert!(missing.to_string().contains("require image"));

    let invalid = presentation::create_pptx(
        &json!({
            "target_path":"invalid.pptx",
            "slides":[{
                "title":"Image",
                "layout":"image_full",
                "image":{"path":"fake.png","alt_text":"Invalid"}
            }]
        }),
        &state,
        &request,
    )
    .expect_err("invalid image must fail");
    assert!(invalid.to_string().contains("invalid signature"));
    assert!(!root.join("invalid.pptx").exists());

    let controls = presentation::create_pptx(
        &json!({
            "target_path":"controls.pptx",
            "slides":[{"title":"Bad\u{0}Title","body":"Body"}]
        }),
        &state,
        &request,
    )
    .expect_err("XML controls must fail");
    assert!(controls.to_string().contains("control characters"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_appends_and_replaces_structured_docx_content() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/structured.docx",
            "blocks":[
                {"type":"paragraph","style":"title","text":"Quarterly Report"},
                {"type":"paragraph","style":"heading1","text":"Overview"},
                {"type":"paragraph","text":"Alpha result"},
                {"type":"table","header_row":true,"rows":[["Metric","Value"],["Users","42"]]},
                {"type":"page_break"},
                {"type":"paragraph","style":"quote","text":"Keep the original wording."}
            ]
        }),
        &state,
        &request,
    )
    .expect("structured DOCX");
    let inspected = inspect_docx(
        &json!({"path":"artifacts/structured.docx"}),
        &state,
        &request,
    )
    .expect("inspect structured DOCX");
    assert_eq!(inspected.get("tables").and_then(Value::as_u64), Some(1));
    assert_eq!(
        inspected.get("page_breaks").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(inspected.get("headings").and_then(Value::as_u64), Some(1));

    let source_before = fs::read(root.join("artifacts/structured.docx")).expect("source bytes");
    docx_edit::append_docx_content(
        &json!({
            "path":"artifacts/structured.docx",
            "target_path":"artifacts/appended.docx",
            "blocks":[
                {"type":"paragraph","style":"heading2","text":"Appendix"},
                {"type":"paragraph","text":"Appended locally."}
            ]
        }),
        &state,
        &request,
    )
    .expect("append DOCX");
    let appended = inspect_docx(&json!({"path":"artifacts/appended.docx"}), &state, &request)
        .expect("inspect appended DOCX");
    assert!(appended
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Appendix") && text.contains("Appended locally.")));
    let mut appended_archive = ZipArchive::new(
        std::fs::File::open(root.join("artifacts/appended.docx")).expect("appended file"),
    )
    .expect("appended archive");
    assert!(appended_archive.by_name("word/styles.xml").is_ok());

    docx_edit::replace_docx_text(
        &json!({
            "path":"artifacts/appended.docx",
            "find":"Alpha",
            "replace":"Beta",
            "target_path":"artifacts/replaced.docx"
        }),
        &state,
        &request,
    )
    .expect("replace DOCX text");
    let replaced = inspect_docx(&json!({"path":"artifacts/replaced.docx"}), &state, &request)
        .expect("inspect replaced DOCX");
    let preview = replaced
        .get("text_preview")
        .and_then(Value::as_str)
        .expect("text preview");
    assert!(preview.contains("Beta result"));
    assert!(!preview.contains("Alpha result"));
    assert_eq!(
        fs::read(root.join("artifacts/structured.docx")).expect("source after edit"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_one_exact_simple_docx_table_cell_without_modifying_the_source() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Quarterly metrics","style":"heading1"},
                {"type":"table","rows":[["Do","Not Edit"]]},
                {"type":"table","header_row":true,"rows":[["Metric","Value"],["Users","42"],["Errors","3"]]}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source = root.join("artifacts/source.docx");
    let source_before = fs::read(source.as_path()).expect("source DOCX bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source.as_path()).expect("source DOCX")).expect("source ZIP");
    let source_xml =
        read_zip_text(&mut source_archive, "word/document.xml").expect("source document XML");

    let replaced = docx_edit::replace_docx_table_cell_text(
        &json!({
            "path":"artifacts/source.docx",
            "table":2,
            "row":2,
            "column":2,
            "expected_text":"42",
            "replacement":"43 & verified",
            "target_path":"artifacts/updated.docx"
        }),
        &state,
        &request,
    )
    .expect("replace table cell");
    assert_eq!(
        replaced.get("operation").and_then(Value::as_str),
        Some("replace_table_cell_text")
    );
    assert_eq!(replaced.get("table").and_then(Value::as_u64), Some(2));
    assert_eq!(replaced.get("row").and_then(Value::as_u64), Some(2));
    assert_eq!(replaced.get("column").and_then(Value::as_u64), Some(2));
    assert_eq!(
        replaced
            .get("formatting_preserved")
            .and_then(Value::as_bool),
        Some(true)
    );

    let mut updated_archive =
        ZipArchive::new(File::open(root.join("artifacts/updated.docx")).expect("updated DOCX"))
            .expect("updated ZIP");
    let updated_xml =
        read_zip_text(&mut updated_archive, "word/document.xml").expect("updated document XML");
    assert_eq!(
        updated_xml,
        source_xml.replacen(">42<", ">43 &amp; verified<", 1)
    );
    let inspected = inspect_docx(&json!({"path":"artifacts/updated.docx"}), &state, &request)
        .expect("inspect updated DOCX");
    assert!(inspected
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("43 & verified") && text.contains("Errors") && text.contains('3')
        }));
    assert_eq!(
        fs::read(source.as_path()).expect("source DOCX after edit"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_table_cell_replacement_rejects_mismatch_noop_complex_and_in_place_edits() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"source.docx",
            "blocks":[{"type":"table","rows":[["A","B"],["C","D"]]}]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"source.docx","table":1,"row":2,"column":2,"expected_text":"wrong","replacement":"X","target_path":"mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            1,
            json!({"path":"source.docx","table":1,"row":2,"column":2,"expected_text":"D","replacement":"D","target_path":"noop.docx"}),
            "must differ",
        ),
        (
            2,
            json!({"path":"source.docx","table":1,"row":3,"column":1,"expected_text":"D","replacement":"X","target_path":"outside.docx"}),
            "row index 3",
        ),
        (
            3,
            json!({"path":"source.docx","table":1,"row":2,"column":2,"expected_text":"D","replacement":"X","target_path":"source.docx","overwrite":true}),
            "distinct target_path",
        ),
    ] {
        let error = docx_edit::replace_docx_table_cell_text(&arguments, &state, &request)
            .expect_err("unsafe table cell edit must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    let merged_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("merged.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), merged_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ],
        false,
    )
    .expect("merged-cell DOCX");
    let merged_error = docx_edit::replace_docx_table_cell_text(
        &json!({"path":"merged.docx","table":1,"row":1,"column":1,"expected_text":"Merged","replacement":"X","target_path":"merged-output.docx"}),
        &state,
        &request,
    )
    .expect_err("merged cell must fail closed");
    assert!(merged_error.to_string().contains("merged or complex"));

    assert_eq!(
        fs::read(root.join("source.docx")).expect("source DOCX after rejections"),
        source_before
    );
    for target in [
        "mismatch.docx",
        "noop.docx",
        "outside.docx",
        "merged-output.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deletes_one_exact_simple_docx_table_row_without_modifying_the_source() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Quarterly metrics","style":"heading1"},
                {"type":"table","rows":[["Do","Not Delete"]]},
                {"type":"table","header_row":true,"rows":[["Metric","Value"],["Users","42"],["Errors","3"]]},
                {"type":"paragraph","text":"Summary remains"}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source = root.join("artifacts/source.docx");
    let source_before = fs::read(source.as_path()).expect("source DOCX bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source.as_path()).expect("source DOCX")).expect("source ZIP");
    let source_xml =
        read_zip_text(&mut source_archive, "word/document.xml").expect("source document XML");
    let source_styles =
        read_zip_text(&mut source_archive, "word/styles.xml").expect("source styles XML");
    let source_content_types =
        read_zip_text(&mut source_archive, "[Content_Types].xml").expect("content types");
    drop(source_archive);
    let users = source_xml.find(">Users<").expect("Users cell");
    let row_start = source_xml[..users].rfind("<w:tr>").expect("row start");
    let row_end = users + source_xml[users..].find("</w:tr>").expect("row end") + "</w:tr>".len();
    let expected_xml = format!("{}{}", &source_xml[..row_start], &source_xml[row_end..]);

    let deleted = docx_edit::delete_docx_table_row(
        &json!({
            "path":"artifacts/source.docx",
            "table":2,
            "row":2,
            "expected_cells":["Users","42"],
            "target_path":"artifacts/updated.docx"
        }),
        &state,
        &request,
    )
    .expect("delete table row");
    assert_eq!(
        deleted.get("operation").and_then(Value::as_str),
        Some("delete_table_row")
    );
    assert_eq!(deleted.get("table").and_then(Value::as_u64), Some(2));
    assert_eq!(deleted.get("row").and_then(Value::as_u64), Some(2));
    assert_eq!(
        deleted.get("removed_cells").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(deleted.get("rows_before").and_then(Value::as_u64), Some(3));
    assert_eq!(deleted.get("rows_after").and_then(Value::as_u64), Some(2));

    let mut updated_archive =
        ZipArchive::new(File::open(root.join("artifacts/updated.docx")).expect("updated DOCX"))
            .expect("updated ZIP");
    assert_eq!(
        read_zip_text(&mut updated_archive, "word/document.xml").expect("updated document XML"),
        expected_xml
    );
    assert_eq!(
        read_zip_text(&mut updated_archive, "word/styles.xml").expect("updated styles XML"),
        source_styles
    );
    assert_eq!(
        read_zip_text(&mut updated_archive, "[Content_Types].xml").expect("updated content types"),
        source_content_types
    );
    drop(updated_archive);
    let inspected = inspect_docx(&json!({"path":"artifacts/updated.docx"}), &state, &request)
        .expect("inspect updated DOCX");
    assert!(inspected
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Do")
                && text.contains("Not Delete")
                && text.contains("Errors")
                && text.contains('3')
                && text.contains("Summary remains")
                && !text.contains("Users")
                && !text.contains("42")
        }));
    assert_eq!(
        fs::read(source.as_path()).expect("source DOCX after row deletion"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_table_row_deletion_rejects_mismatch_last_row_complex_ranges_and_in_place_edits() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"source.docx",
            "blocks":[{"type":"table","rows":[["A","B"],["C","D"]]}]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"source.docx","table":1,"row":2,"expected_cells":["C","wrong"],"target_path":"mismatch.docx"}),
            "do not match expected_cells",
        ),
        (
            1,
            json!({"path":"source.docx","table":1,"row":3,"expected_cells":["C","D"],"target_path":"outside.docx"}),
            "row index 3",
        ),
        (
            2,
            json!({"path":"source.docx","table":1,"row":2,"expected_cells":["C","D"],"target_path":"source.docx","overwrite":true}),
            "distinct target_path",
        ),
        (
            3,
            json!({"path":"source.docx","table":1,"row":2,"expected_cells":[],"target_path":"empty-expected.docx"}),
            "expected_cells must contain",
        ),
    ] {
        let error = docx_edit::delete_docx_table_row(&arguments, &state, &request)
            .expect_err("unsafe table row deletion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    docx_edit::create_structured_docx(
        &json!({
            "target_path":"single-row.docx",
            "blocks":[{"type":"table","rows":[["Only"]]}]
        }),
        &state,
        &request,
    )
    .expect("single-row DOCX");
    let last_row = docx_edit::delete_docx_table_row(
        &json!({"path":"single-row.docx","table":1,"row":1,"expected_cells":["Only"],"target_path":"last-row.docx"}),
        &state,
        &request,
    )
    .expect_err("deleting the only table row must fail");
    assert!(last_row.to_string().contains("only row"));

    let merged_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Keep</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    let range_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:bookmarkStart w:id="1" w:name="range"/><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:bookmarkEnd w:id="1"/><w:sectPr/></w:body></w:document>"#;
    let malformed_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:document>"#;
    for (source, xml, expected) in [
        ("merged.docx", merged_xml, "merged or complex"),
        ("range.docx", range_xml, "document range markup"),
        (
            "malformed.docx",
            malformed_xml,
            "mismatched element boundaries",
        ),
    ] {
        write_zip(
            root.join(source).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                ("word/document.xml".to_string(), xml.to_string()),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("unsafe table DOCX");
        let error = docx_edit::delete_docx_table_row(
            &json!({"path":source,"table":1,"row":1,"expected_cells":[if source == "merged.docx" {"Merged"} else {"A"}],"target_path":format!("{source}.output.docx")}),
            &state,
            &request,
        )
        .expect_err("unsafe table structure must fail closed");
        assert!(error.to_string().contains(expected), "{source}: {error:#}");
    }

    assert_eq!(
        fs::read(root.join("source.docx")).expect("source DOCX after rejections"),
        source_before
    );
    for target in [
        "mismatch.docx",
        "outside.docx",
        "empty-expected.docx",
        "last-row.docx",
        "merged.docx.output.docx",
        "range.docx.output.docx",
        "malformed.docx.output.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_simple_docx_table_rows_with_cloned_formatting_and_stripped_identity_attributes() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"><w:body><w:p><w:r><w:t>Before table</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tr><w:tc><w:p><w:r><w:t>First</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Row</w:t></w:r></w:p></w:tc></w:tr><w:tr w:rsidR="00112233"><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:shd w:val="clear" w:fill="D9EAF7"/></w:tcPr><w:p w14:paraId="00000001" w14:textId="11111111"><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Alpha</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:shd w:val="clear" w:fill="D9EAF7"/></w:tcPr><w:p w14:paraId="00000002" w14:textId="22222222"><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Beta</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After table</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>unchanged</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");

    let inserted_after = docx_edit::insert_docx_table_row(
        &json!({
            "path":"source.docx",
            "table":1,
            "reference_row":2,
            "position":"after",
            "expected_cells":["Alpha","Beta"],
            "cells":[" Gamma & ","Delta"],
            "target_path":"after.docx"
        }),
        &state,
        &request,
    )
    .expect("insert row after reference");
    assert_eq!(
        inserted_after.get("operation").and_then(Value::as_str),
        Some("insert_table_row")
    );
    assert_eq!(
        inserted_after.get("reference_row").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        inserted_after.get("inserted_row").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        inserted_after.get("rows_before").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        inserted_after.get("rows_after").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        inserted_after
            .get("stripped_identity_attributes")
            .and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        inserted_after
            .get("formatting_cloned")
            .and_then(Value::as_bool),
        Some(true)
    );

    let mut after_archive =
        ZipArchive::new(File::open(root.join("after.docx")).expect("after DOCX"))
            .expect("after ZIP");
    let after_xml =
        read_zip_text(&mut after_archive, "word/document.xml").expect("after document XML");
    assert_eq!(after_xml.matches("w14:paraId=").count(), 2);
    assert_eq!(after_xml.matches("w14:textId=").count(), 2);
    assert_eq!(after_xml.matches("<w:cantSplit/>").count(), 2);
    assert_eq!(after_xml.matches("<w:shd ").count(), 4);
    assert_eq!(after_xml.matches("<w:b/>").count(), 4);
    assert!(after_xml.contains("<w:t xml:space=\"preserve\"> Gamma &amp; </w:t>"));
    assert!(after_xml.find(">Alpha<").expect("Alpha") < after_xml.find(">Delta<").expect("Delta"));
    assert_eq!(
        read_zip_text(&mut after_archive, "word/custom.xml").expect("custom XML"),
        "<custom>unchanged</custom>"
    );

    let inserted_before = docx_edit::insert_docx_table_row(
        &json!({
            "path":"source.docx",
            "table":1,
            "reference_row":2,
            "position":"before",
            "expected_cells":["Alpha","Beta"],
            "cells":["Before Alpha","Before Beta"],
            "target_path":"before.docx"
        }),
        &state,
        &request,
    )
    .expect("insert row before reference");
    assert_eq!(
        inserted_before.get("inserted_row").and_then(Value::as_u64),
        Some(2)
    );
    let mut before_archive =
        ZipArchive::new(File::open(root.join("before.docx")).expect("before DOCX"))
            .expect("before ZIP");
    let before_xml =
        read_zip_text(&mut before_archive, "word/document.xml").expect("before document XML");
    assert!(
        before_xml.find(">Before Alpha<").expect("inserted before")
            < before_xml.find(">Alpha<").expect("reference Alpha")
    );
    assert_eq!(
        fs::read(root.join("source.docx")).expect("source after insertions"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_table_row_insertion_rejects_mismatch_counts_header_complex_ranges_and_in_place_edits() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"source.docx",
            "blocks":[{"type":"table","rows":[["A","B"],["C","D"]]}]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"source.docx","table":1,"reference_row":2,"position":"after","expected_cells":["C","wrong"],"cells":["E","F"],"target_path":"mismatch.docx"}),
            "do not match expected_cells",
        ),
        (
            1,
            json!({"path":"source.docx","table":1,"reference_row":2,"position":"after","expected_cells":["C","D"],"cells":["E"],"target_path":"count.docx"}),
            "same number of items",
        ),
        (
            2,
            json!({"path":"source.docx","table":1,"reference_row":3,"position":"after","expected_cells":["C","D"],"cells":["E","F"],"target_path":"outside.docx"}),
            "reference_row index 3",
        ),
        (
            3,
            json!({"path":"source.docx","table":1,"reference_row":2,"position":"middle","expected_cells":["C","D"],"cells":["E","F"],"target_path":"position.docx"}),
            "before or after",
        ),
        (
            4,
            json!({"path":"source.docx","table":1,"reference_row":2,"position":"after","expected_cells":["C","D"],"cells":["E","F"],"target_path":"source.docx","overwrite":true}),
            "distinct target_path",
        ),
        (
            5,
            json!({"path":"source.docx","table":1,"reference_row":2,"position":"after","expected_cells":["C","D"],"cells":[],"target_path":"empty.docx"}),
            "cells must contain",
        ),
    ] {
        let error = docx_edit::insert_docx_table_row(&arguments, &state, &request)
            .expect_err("unsafe table row insertion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    let header_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    let merged_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    let range_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:bookmarkStart w:id="1" w:name="range"/><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:bookmarkEnd w:id="1"/><w:sectPr/></w:body></w:document>"#;
    let unsupported_text_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t w:foo="bar">A</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    for (source, xml, expected_cells, expected) in [
        (
            "header.docx",
            header_xml,
            vec!["Header"],
            "repeating table header",
        ),
        (
            "merged.docx",
            merged_xml,
            vec!["Merged"],
            "merged or complex",
        ),
        ("range.docx", range_xml, vec!["A"], "document range markup"),
        (
            "unsupported-text.docx",
            unsupported_text_xml,
            vec!["A"],
            "standard w:t opening tags",
        ),
    ] {
        write_zip(
            root.join(source).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                ("word/document.xml".to_string(), xml.to_string()),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("unsafe table DOCX");
        let error = docx_edit::insert_docx_table_row(
            &json!({
                "path":source,
                "table":1,
                "reference_row":1,
                "position":"after",
                "expected_cells":expected_cells,
                "cells":["New"],
                "target_path":format!("{source}.output.docx")
            }),
            &state,
            &request,
        )
        .expect_err("unsafe table structure must fail closed");
        assert!(error.to_string().contains(expected), "{source}: {error:#}");
    }

    assert_eq!(
        fs::read(root.join("source.docx")).expect("source DOCX after rejections"),
        source_before
    );
    for target in [
        "mismatch.docx",
        "count.docx",
        "outside.docx",
        "position.docx",
        "empty.docx",
        "header.docx.output.docx",
        "merged.docx.output.docx",
        "range.docx.output.docx",
        "unsupported-text.docx.output.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn moves_simple_docx_table_rows_without_rewriting_row_or_package_content() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tr w:rsidR="11111111"><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:shd w:val="clear" w:fill="D9EAF7"/></w:tcPr><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Alpha</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Beta</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Gamma</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p></w:tc></w:tr><w:tr w:rsidR="44444444"><w:tc><w:p><w:r><w:t>Delta</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>4</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>unchanged</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");

    let alpha_text = document_xml.find(">Alpha<").expect("Alpha cell");
    let alpha_start = document_xml[..alpha_text]
        .rfind("<w:tr ")
        .expect("Alpha row");
    let alpha_end = alpha_text
        + document_xml[alpha_text..]
            .find("</w:tr>")
            .expect("Alpha row end")
        + "</w:tr>".len();
    let gamma_text = document_xml.find(">Gamma<").expect("Gamma cell");
    let gamma_end = gamma_text
        + document_xml[gamma_text..]
            .find("</w:tr>")
            .expect("Gamma row end")
        + "</w:tr>".len();
    let alpha_row = &document_xml[alpha_start..alpha_end];
    let expected_after = format!(
        "{}{}{}{}",
        &document_xml[..alpha_start],
        &document_xml[alpha_end..gamma_end],
        alpha_row,
        &document_xml[gamma_end..]
    );
    let moved_after = docx_edit::move_docx_table_row(
        &json!({
            "path":"source.docx",
            "table":1,
            "row":1,
            "expected_cells":["Alpha","1"],
            "reference_row":3,
            "reference_expected_cells":["Gamma","3"],
            "position":"after",
            "target_path":"after.docx"
        }),
        &state,
        &request,
    )
    .expect("move Alpha after Gamma");
    assert_eq!(
        moved_after.get("operation").and_then(Value::as_str),
        Some("move_table_row")
    );
    assert_eq!(moved_after.get("row").and_then(Value::as_u64), Some(1));
    assert_eq!(
        moved_after.get("reference_row").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        moved_after.get("moved_row").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        moved_after.get("rows_before").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        moved_after.get("rows_after").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        moved_after
            .get("formatting_preserved")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut after_archive =
        ZipArchive::new(File::open(root.join("after.docx")).expect("after DOCX"))
            .expect("after ZIP");
    assert_eq!(
        read_zip_text(&mut after_archive, "word/document.xml").expect("after document XML"),
        expected_after
    );
    assert_eq!(
        read_zip_text(&mut after_archive, "word/custom.xml").expect("after custom XML"),
        "<custom>unchanged</custom>"
    );

    let beta_text = document_xml.find(">Beta<").expect("Beta cell");
    let beta_start = document_xml[..beta_text].rfind("<w:tr>").expect("Beta row");
    let delta_text = document_xml.find(">Delta<").expect("Delta cell");
    let delta_start = document_xml[..delta_text]
        .rfind("<w:tr ")
        .expect("Delta row");
    let delta_end = delta_text
        + document_xml[delta_text..]
            .find("</w:tr>")
            .expect("Delta row end")
        + "</w:tr>".len();
    let delta_row = &document_xml[delta_start..delta_end];
    let expected_before = format!(
        "{}{}{}{}",
        &document_xml[..beta_start],
        delta_row,
        &document_xml[beta_start..delta_start],
        &document_xml[delta_end..]
    );
    let moved_before = docx_edit::move_docx_table_row(
        &json!({
            "path":"source.docx",
            "table":1,
            "row":4,
            "expected_cells":["Delta","4"],
            "reference_row":2,
            "reference_expected_cells":["Beta","2"],
            "position":"before",
            "target_path":"before.docx"
        }),
        &state,
        &request,
    )
    .expect("move Delta before Beta");
    assert_eq!(
        moved_before.get("moved_row").and_then(Value::as_u64),
        Some(2)
    );
    let mut before_archive =
        ZipArchive::new(File::open(root.join("before.docx")).expect("before DOCX"))
            .expect("before ZIP");
    assert_eq!(
        read_zip_text(&mut before_archive, "word/document.xml").expect("before document XML"),
        expected_before
    );
    assert_eq!(
        read_zip_text(&mut before_archive, "word/custom.xml").expect("before custom XML"),
        "<custom>unchanged</custom>"
    );
    assert_eq!(
        fs::read(root.join("source.docx")).expect("source after row moves"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_table_row_movement_rejects_mismatch_noop_header_complex_ranges_and_in_place_edits() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"source.docx",
            "blocks":[{"type":"table","rows":[["A","1"],["B","2"],["C","3"]]}]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["wrong","1"],"reference_row":3,"reference_expected_cells":["C","3"],"position":"after","target_path":"source-mismatch.docx"}),
            "do not match expected_cells",
        ),
        (
            1,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":3,"reference_expected_cells":["wrong","3"],"position":"after","target_path":"reference-mismatch.docx"}),
            "do not match reference_expected_cells",
        ),
        (
            2,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":1,"reference_expected_cells":["A","1"],"position":"before","target_path":"same.docx"}),
            "different rows",
        ),
        (
            3,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":2,"reference_expected_cells":["B","2"],"position":"before","target_path":"noop-before.docx"}),
            "already in the requested position",
        ),
        (
            4,
            json!({"path":"source.docx","table":1,"row":2,"expected_cells":["B","2"],"reference_row":1,"reference_expected_cells":["A","1"],"position":"after","target_path":"noop-after.docx"}),
            "already in the requested position",
        ),
        (
            5,
            json!({"path":"source.docx","table":1,"row":4,"expected_cells":["A","1"],"reference_row":1,"reference_expected_cells":["A","1"],"position":"after","target_path":"row-outside.docx"}),
            "row index 4",
        ),
        (
            6,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":4,"reference_expected_cells":["C","3"],"position":"after","target_path":"reference-outside.docx"}),
            "reference_row index 4",
        ),
        (
            7,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":3,"reference_expected_cells":["C","3"],"position":"middle","target_path":"position.docx"}),
            "before or after",
        ),
        (
            8,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":3,"reference_expected_cells":["C","3"],"position":"after","target_path":"source.docx","overwrite":true}),
            "distinct target_path",
        ),
        (
            9,
            json!({"path":"source.docx","table":1,"row":1,"expected_cells":["A","1"],"reference_row":3,"reference_expected_cells":[],"position":"after","target_path":"empty-reference.docx"}),
            "reference_expected_cells must contain",
        ),
    ] {
        let error = docx_edit::move_docx_table_row(&arguments, &state, &request)
            .expect_err("unsafe table row movement must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    let header_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Tail</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    let merged_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Middle</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Tail</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    let range_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:bookmarkStart w:id="1" w:name="range"/><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:bookmarkEnd w:id="1"/><w:sectPr/></w:body></w:document>"#;
    let revision_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:ins w:id="1"><w:r><w:t>A</w:t></w:r></w:ins></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr/></w:body></w:document>"#;
    for (source, xml, row, expected_cells, reference_row, reference_cells, expected) in [
        (
            "header.docx",
            header_xml,
            1,
            vec!["Header"],
            3,
            vec!["Tail"],
            "repeating table header",
        ),
        (
            "header-reference.docx",
            header_xml,
            3,
            vec!["Tail"],
            1,
            vec!["Header"],
            "repeating table header",
        ),
        (
            "merged.docx",
            merged_xml,
            1,
            vec!["Merged"],
            3,
            vec!["Tail"],
            "merged or complex",
        ),
        (
            "range.docx",
            range_xml,
            1,
            vec!["A"],
            3,
            vec!["C"],
            "document range markup",
        ),
        (
            "revision.docx",
            revision_xml,
            1,
            vec!["A"],
            3,
            vec!["C"],
            "revision or structured-content markup",
        ),
    ] {
        write_zip(
            root.join(source).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                ("word/document.xml".to_string(), xml.to_string()),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("unsafe table DOCX");
        let error = docx_edit::move_docx_table_row(
            &json!({
                "path":source,
                "table":1,
                "row":row,
                "expected_cells":expected_cells,
                "reference_row":reference_row,
                "reference_expected_cells":reference_cells,
                "position":"after",
                "target_path":format!("{source}.output.docx")
            }),
            &state,
            &request,
        )
        .expect_err("unsafe table structure must fail closed");
        assert!(error.to_string().contains(expected), "{source}: {error:#}");
    }

    assert_eq!(
        fs::read(root.join("source.docx")).expect("source DOCX after rejections"),
        source_before
    );
    for target in [
        "source-mismatch.docx",
        "reference-mismatch.docx",
        "same.docx",
        "noop-before.docx",
        "noop-after.docx",
        "row-outside.docx",
        "reference-outside.docx",
        "position.docx",
        "empty-reference.docx",
        "header.docx.output.docx",
        "header-reference.docx.output.docx",
        "merged.docx.output.docx",
        "range.docx.output.docx",
        "revision.docx.output.docx",
    ] {
        assert!(!root.join(target).exists(), "unexpected output {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_docx_image_and_adds_header_footer_without_modifying_sources() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[{"type":"paragraph","style":"title","text":"Visual report"}]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::write(root.join("artifacts/pixel.png"), image).expect("write test PNG");
    let source_before = fs::read(root.join("artifacts/source.docx")).expect("source bytes");

    let inserted = docx_edit::insert_docx_image(
        &json!({
            "path":"artifacts/source.docx",
            "image_path":"artifacts/pixel.png",
            "target_path":"artifacts/with-image.docx",
            "width_inches":2.0,
            "alt_text":"One pixel chart",
            "align":"right"
        }),
        &state,
        &request,
    )
    .expect("insert DOCX image");
    assert_eq!(inserted.get("pixel_width").and_then(Value::as_u64), Some(1));
    assert_eq!(
        inserted.get("pixel_height").and_then(Value::as_u64),
        Some(1)
    );
    let image_inspection = inspect_docx(
        &json!({"path":"artifacts/with-image.docx"}),
        &state,
        &request,
    )
    .expect("inspect image DOCX");
    assert_eq!(
        image_inspection.get("media_files").and_then(Value::as_u64),
        Some(1)
    );
    let image_source_before =
        fs::read(root.join("artifacts/with-image.docx")).expect("image DOCX bytes");
    let mut image_archive =
        ZipArchive::new(File::open(root.join("artifacts/with-image.docx")).expect("image DOCX"))
            .expect("image archive");
    let image_document =
        read_zip_text(&mut image_archive, "word/document.xml").expect("image document XML");
    let image_relationships = read_zip_text(&mut image_archive, "word/_rels/document.xml.rels")
        .expect("image relationships");
    assert!(image_document.contains("One pixel chart"));
    assert!(image_document.contains("<w:drawing"));
    assert!(image_relationships.contains("relationships/image"));

    docx_edit::add_docx_header_footer(
        &json!({
            "path":"artifacts/with-image.docx",
            "target_path":"artifacts/complete.docx",
            "header_text":"Confidential\nFY 2026",
            "footer_text":"Prepared locally",
            "header_align":"left",
            "footer_align":"right"
        }),
        &state,
        &request,
    )
    .expect("add DOCX header/footer");
    let complete = inspect_docx(&json!({"path":"artifacts/complete.docx"}), &state, &request)
        .expect("inspect complete DOCX");
    assert_eq!(complete.get("headers").and_then(Value::as_u64), Some(1));
    assert_eq!(complete.get("footers").and_then(Value::as_u64), Some(1));
    assert!(complete
        .get("header_text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Confidential") && text.contains("FY 2026")));
    assert!(complete
        .get("footer_text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Prepared locally")));
    assert_eq!(
        fs::read(root.join("artifacts/source.docx")).expect("source after image"),
        source_before
    );
    assert_eq!(
        fs::read(root.join("artifacts/with-image.docx")).expect("image source after header"),
        image_source_before
    );

    let duplicate_header = docx_edit::add_docx_header_footer(
        &json!({
            "path":"artifacts/complete.docx",
            "target_path":"artifacts/duplicate-header.docx",
            "header_text":"Replacement"
        }),
        &state,
        &request,
    )
    .expect_err("existing header reference must fail closed");
    assert!(duplicate_header
        .to_string()
        .contains("already contains header references"));
    assert!(!root.join("artifacts/duplicate-header.docx").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_referenced_docx_header_text_without_modifying_footer_relationships_or_source() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"base.docx",
            "blocks":[{"type":"paragraph","style":"title","text":"Quarterly report"}]
        }),
        &state,
        &request,
    )
    .expect("create base DOCX");
    docx_edit::add_docx_header_footer(
        &json!({
            "path":"base.docx",
            "target_path":"source.docx",
            "header_text":"Confidential Confidential",
            "footer_text":"Confidential footer",
            "header_align":"left",
            "footer_align":"right"
        }),
        &state,
        &request,
    )
    .expect("add header and footer");

    let source_path = root.join("source.docx");
    let source_before = fs::read(source_path.as_path()).expect("source bytes");
    let mut source_archive =
        ZipArchive::new(File::open(source_path.as_path()).expect("source file"))
            .expect("source archive");
    let document_before =
        read_zip_text(&mut source_archive, "word/document.xml").expect("document XML");
    let relationships_before = read_zip_text(&mut source_archive, "word/_rels/document.xml.rels")
        .expect("document relationships");
    let content_types_before =
        read_zip_text(&mut source_archive, "[Content_Types].xml").expect("content types");
    let header_before = read_zip_text(&mut source_archive, "word/header1.xml").expect("header XML");
    let footer_before = read_zip_text(&mut source_archive, "word/footer1.xml").expect("footer XML");
    let header_run_properties = header_before.matches("<w:rPr").count();
    drop(source_archive);

    let updated = docx_edit::replace_docx_header_footer_text(
        &json!({
            "path":"source.docx",
            "target_path":"updated.docx",
            "find":"Confidential",
            "replacement":"Internal & Approved",
            "part_names":["word/header1.xml"],
            "max_replacements":1
        }),
        &state,
        &request,
    )
    .expect("replace referenced header text");
    assert_eq!(
        updated.get("selected_parts"),
        Some(&json!(["word/header1.xml"]))
    );
    assert_eq!(
        updated.get("matched_parts"),
        Some(&json!(["word/header1.xml"]))
    );
    assert_eq!(
        updated.get("matched_headers").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        updated.get("matched_footers").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(updated.get("replacements").and_then(Value::as_u64), Some(1));
    assert_eq!(
        updated
            .get("replacement_limit_reached")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fs::read(source_path.as_path()).expect("source after replacement"),
        source_before
    );

    let inspected = inspect_docx(&json!({"path":"updated.docx"}), &state, &request)
        .expect("inspect updated DOCX");
    assert_eq!(
        inspected.get("header_parts"),
        Some(&json!(["word/header1.xml"]))
    );
    assert_eq!(
        inspected.get("footer_parts"),
        Some(&json!(["word/footer1.xml"]))
    );
    assert!(inspected
        .get("header_text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text == "Internal & Approved Confidential"));
    assert_eq!(
        inspected.get("footer_text_preview").and_then(Value::as_str),
        Some("Confidential footer")
    );

    let mut output_archive =
        ZipArchive::new(File::open(root.join("updated.docx")).expect("output file"))
            .expect("output archive");
    assert_eq!(
        read_zip_text(&mut output_archive, "word/document.xml").expect("preserved document"),
        document_before
    );
    assert_eq!(
        read_zip_text(&mut output_archive, "word/_rels/document.xml.rels")
            .expect("preserved relationships"),
        relationships_before
    );
    assert_eq!(
        read_zip_text(&mut output_archive, "[Content_Types].xml").expect("preserved content types"),
        content_types_before
    );
    assert_eq!(
        read_zip_text(&mut output_archive, "word/footer1.xml").expect("preserved footer"),
        footer_before
    );
    let header_after =
        read_zip_text(&mut output_archive, "word/header1.xml").expect("updated header");
    assert!(header_after.contains("Internal &amp; Approved Confidential"));
    assert_eq!(
        header_after.matches("<w:rPr").count(),
        header_run_properties
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_header_footer_replacement_rejects_missing_cross_run_unknown_duplicate_and_in_place() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"plain.docx",
            "blocks":[{"type":"paragraph","text":"Body"}]
        }),
        &state,
        &request,
    )
    .expect("create plain DOCX");
    let missing = docx_edit::replace_docx_header_footer_text(
        &json!({
            "path":"plain.docx",
            "target_path":"missing.docx",
            "find":"Body",
            "replacement":"Changed"
        }),
        &state,
        &request,
    )
    .expect_err("missing header/footer references must fail");
    assert!(missing
        .to_string()
        .contains("no referenced header or footer parts"));
    assert!(!root.join("missing.docx").exists());

    docx_edit::add_docx_header_footer(
        &json!({
            "path":"plain.docx",
            "target_path":"source.docx",
            "header_text":"Alpha\nBeta",
            "footer_text":"Footer"
        }),
        &state,
        &request,
    )
    .expect("add split-run header");
    let source_before = fs::read(root.join("source.docx")).expect("source bytes");
    for (target, part_names, find, expected) in [
        (
            "cross-run.docx",
            json!(["word/header1.xml"]),
            "AlphaBeta",
            "selected header/footer DOCX text run",
        ),
        (
            "unknown-part.docx",
            json!(["word/document.xml"]),
            "Alpha",
            "not a referenced DOCX header or footer",
        ),
        (
            "duplicate-part.docx",
            json!(["word/header1.xml", "word/header1.xml"]),
            "Alpha",
            "must not contain duplicates",
        ),
    ] {
        let error = docx_edit::replace_docx_header_footer_text(
            &json!({
                "path":"source.docx",
                "target_path":target,
                "find":find,
                "replacement":"Changed",
                "part_names":part_names
            }),
            &state,
            &request,
        )
        .expect_err("unsafe header/footer replacement must fail");
        assert!(error.to_string().contains(expected));
        assert!(!root.join(target).exists());
    }

    let in_place = docx_edit::replace_docx_header_footer_text(
        &json!({
            "path":"source.docx",
            "target_path":"source.docx",
            "find":"Alpha",
            "replacement":"Changed"
        }),
        &state,
        &request,
    )
    .expect_err("in-place header/footer replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("source.docx")).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_image_rejects_invalid_content_and_in_place_output() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({"target_path":"source.docx","paragraphs":["Hello"]}),
        &state,
        &request,
    )
    .expect("source DOCX");
    fs::write(root.join("fake.png"), b"not a png").expect("fake PNG");
    let invalid = docx_edit::insert_docx_image(
        &json!({
            "path":"source.docx",
            "image_path":"fake.png",
            "target_path":"invalid.docx"
        }),
        &state,
        &request,
    )
    .expect_err("invalid PNG must fail");
    assert!(invalid.to_string().contains("invalid signature"));
    assert!(!root.join("invalid.docx").exists());

    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::write(root.join("pixel.png"), image).expect("write test PNG");
    let in_place = docx_edit::insert_docx_image(
        &json!({
            "path":"source.docx",
            "image_path":"pixel.png",
            "target_path":"source.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place image insertion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn adds_and_appends_exact_run_docx_comments_without_modifying_sources() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Review this sentence."},
                {"type":"paragraph","text":"Confirm the final number."}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("artifacts/source.docx")).expect("source bytes");

    let first = docx_edit::add_docx_comment(
        &json!({
            "path":"artifacts/source.docx",
            "selection":"Review this sentence.",
            "comment":"Add supporting evidence.",
            "author":"Reviewer",
            "initials":"RV",
            "target_path":"artifacts/commented-once.docx"
        }),
        &state,
        &request,
    )
    .expect("first comment");
    assert_eq!(first.get("comment_id").and_then(Value::as_u64), Some(0));
    let first_before =
        fs::read(root.join("artifacts/commented-once.docx")).expect("first comment bytes");
    let mut first_archive = ZipArchive::new(
        File::open(root.join("artifacts/commented-once.docx")).expect("commented DOCX"),
    )
    .expect("commented archive");
    let document_xml =
        read_zip_text(&mut first_archive, "word/document.xml").expect("document XML");
    let comments_xml =
        read_zip_text(&mut first_archive, "word/comments.xml").expect("comments XML");
    let relationships_xml = read_zip_text(&mut first_archive, "word/_rels/document.xml.rels")
        .expect("relationships XML");
    let content_types_xml =
        read_zip_text(&mut first_archive, "[Content_Types].xml").expect("content types XML");
    assert!(document_xml.contains("<w:commentRangeStart w:id=\"0\"/>"));
    assert!(document_xml.contains("<w:commentReference w:id=\"0\"/>"));
    assert!(comments_xml.contains("Add supporting evidence."));
    assert!(comments_xml.contains("w:author=\"Reviewer\""));
    assert!(relationships_xml.contains("relationships/comments"));
    assert!(content_types_xml.contains("/word/comments.xml"));

    let second = docx_edit::add_docx_comment(
        &json!({
            "path":"artifacts/commented-once.docx",
            "selection":"Confirm the final number.",
            "comment":"Verify against the signed source.",
            "target_path":"artifacts/commented-twice.docx"
        }),
        &state,
        &request,
    )
    .expect("second comment");
    assert_eq!(second.get("comment_id").and_then(Value::as_u64), Some(1));
    let inspected = inspect_docx(
        &json!({"path":"artifacts/commented-twice.docx"}),
        &state,
        &request,
    )
    .expect("inspect comments");
    assert_eq!(inspected.get("comments").and_then(Value::as_u64), Some(2));
    assert!(inspected
        .get("comment_text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Add supporting evidence.")
                && text.contains("Verify against the signed source.")
        }));
    assert_eq!(
        fs::read(root.join("artifacts/source.docx")).expect("source after comments"),
        source_before
    );
    assert_eq!(
        fs::read(root.join("artifacts/commented-once.docx")).expect("first after second"),
        first_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_comments_reject_substrings_cross_run_text_and_in_place_output() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p><w:p><w:r><w:t>Complete run</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("split.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ],
        false,
    )
    .expect("split-run DOCX");

    for (selection, target) in [("Hello", "cross-run.docx"), ("Complete", "substring.docx")] {
        let error = docx_edit::add_docx_comment(
            &json!({
                "path":"split.docx",
                "selection":selection,
                "comment":"No guessing",
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("non-whole-run comment must fail closed");
        assert!(error.to_string().contains("complete text"));
        assert!(!root.join(target).exists());
    }

    let in_place = docx_edit::add_docx_comment(
        &json!({
            "path":"split.docx",
            "selection":"Complete run",
            "comment":"No in-place edits",
            "target_path":"split.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place comment must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_tracked_replacement_and_deletion_without_modifying_sources() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Draft wording","bold":true},
                {"type":"paragraph","text":"Remove this sentence"}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    let source_before = fs::read(root.join("artifacts/source.docx")).expect("source bytes");

    let replaced = docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/source.docx",
            "selection":"Draft wording",
            "replacement":"Approved wording",
            "author":"Editor",
            "target_path":"artifacts/replaced.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked replacement");
    assert_eq!(
        replaced.get("deletion_revision_id").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        replaced
            .get("insertion_revision_id")
            .and_then(Value::as_u64),
        Some(1)
    );
    let replaced_inspection =
        inspect_docx(&json!({"path":"artifacts/replaced.docx"}), &state, &request)
            .expect("inspect replacement");
    assert_eq!(
        replaced_inspection
            .get("tracked_insertions")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        replaced_inspection
            .get("tracked_deletions")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert!(replaced_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Approved wording") && !text.contains("Draft wording")));
    let mut replaced_archive =
        ZipArchive::new(File::open(root.join("artifacts/replaced.docx")).expect("replaced DOCX"))
            .expect("replaced archive");
    let replaced_xml =
        read_zip_text(&mut replaced_archive, "word/document.xml").expect("replacement XML");
    assert!(replaced_xml.contains("<w:delText xml:space=\"preserve\">Draft wording</w:delText>"));
    assert!(replaced_xml.contains("<w:ins w:id=\"1\" w:author=\"Editor\""));
    assert!(replaced_xml.contains(">Approved wording</w:t>"));
    assert_eq!(replaced_xml.matches("<w:b/>").count(), 2);

    let deleted = docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/source.docx",
            "selection":"Remove this sentence",
            "replacement":"",
            "target_path":"artifacts/deleted.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked deletion");
    assert!(deleted
        .get("insertion_revision_id")
        .is_some_and(Value::is_null));
    let deleted_inspection =
        inspect_docx(&json!({"path":"artifacts/deleted.docx"}), &state, &request)
            .expect("inspect deletion");
    assert_eq!(
        deleted_inspection
            .get("tracked_insertions")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        deleted_inspection
            .get("tracked_deletions")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert!(!deleted_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Remove this sentence")));
    assert_eq!(
        fs::read(root.join("artifacts/source.docx")).expect("source after revisions"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tracked_replacement_rejects_guessing_nesting_noop_and_in_place_output() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p><w:p><w:r><w:t>Complete run</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ],
        false,
    )
    .expect("source DOCX");

    for (selection, target) in [("Hello", "cross-run.docx"), ("Complete", "substring.docx")] {
        let error = docx_edit::replace_docx_text_tracked(
            &json!({
                "path":"source.docx",
                "selection":selection,
                "replacement":"Changed",
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("tracked replacement guessing must fail");
        assert!(error.to_string().contains("complete text"));
        assert!(!root.join(target).exists());
    }

    let noop = docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"source.docx",
            "selection":"Complete run",
            "replacement":"Complete run",
            "target_path":"noop.docx"
        }),
        &state,
        &request,
    )
    .expect_err("tracked no-op must fail");
    assert!(noop.to_string().contains("must change"));

    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"source.docx",
            "selection":"Complete run",
            "replacement":"Changed run",
            "target_path":"revised.docx"
        }),
        &state,
        &request,
    )
    .expect("first tracked replacement");
    let nested = docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"revised.docx",
            "selection":"Changed run",
            "replacement":"Nested change",
            "target_path":"nested.docx"
        }),
        &state,
        &request,
    )
    .expect_err("nested tracked replacement must fail");
    assert!(nested.to_string().contains("existing tracked revision"));

    let in_place = docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"source.docx",
            "selection":"Complete run",
            "replacement":"Changed run",
            "target_path":"source.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place tracked replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn accepts_and_rejects_simple_docx_tracked_changes_without_modifying_sources() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Draft wording","bold":true},
                {"type":"paragraph","text":"Remove this sentence"}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/source.docx",
            "selection":"Draft wording",
            "replacement":"Approved wording",
            "target_path":"artifacts/replaced.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked replacement");
    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/replaced.docx",
            "selection":"Remove this sentence",
            "replacement":"",
            "target_path":"artifacts/revised.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked deletion");
    let revised_before =
        fs::read(root.join("artifacts/revised.docx")).expect("revised source bytes");

    let accepted = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"artifacts/revised.docx",
            "action":"accept",
            "target_path":"artifacts/accepted.docx"
        }),
        &state,
        &request,
    )
    .expect("accept tracked changes");
    assert_eq!(
        accepted.get("resolved_insertions").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        accepted.get("resolved_deletions").and_then(Value::as_u64),
        Some(2)
    );
    let accepted_inspection =
        inspect_docx(&json!({"path":"artifacts/accepted.docx"}), &state, &request)
            .expect("inspect accepted DOCX");
    assert_eq!(
        accepted_inspection
            .get("tracked_insertions")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        accepted_inspection
            .get("tracked_deletions")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert!(accepted_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Approved wording")
                && !text.contains("Draft wording")
                && !text.contains("Remove this sentence")
        }));

    let rejected = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"artifacts/revised.docx",
            "action":"reject",
            "target_path":"artifacts/rejected.docx"
        }),
        &state,
        &request,
    )
    .expect("reject tracked changes");
    assert_eq!(
        rejected.get("resolved_insertions").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        rejected.get("resolved_deletions").and_then(Value::as_u64),
        Some(2)
    );
    let rejected_inspection =
        inspect_docx(&json!({"path":"artifacts/rejected.docx"}), &state, &request)
            .expect("inspect rejected DOCX");
    assert!(rejected_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Draft wording")
                && text.contains("Remove this sentence")
                && !text.contains("Approved wording")
        }));

    for path in ["artifacts/accepted.docx", "artifacts/rejected.docx"] {
        let mut archive = ZipArchive::new(File::open(root.join(path)).expect("resolved DOCX"))
            .expect("resolved archive");
        let xml = read_zip_text(&mut archive, "word/document.xml").expect("resolved XML");
        assert!(!xml.contains("<w:ins"));
        assert!(!xml.contains("<w:del"));
        assert!(!xml.contains("<w:delText"));
        assert_eq!(xml.matches("<w:b/>").count(), 1);
    }
    assert_eq!(
        fs::read(root.join("artifacts/revised.docx")).expect("revised source after resolve"),
        revised_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspects_and_selectively_resolves_docx_tracked_changes_by_revision_id() {
    let (root, state, request) = test_context();
    docx_edit::create_structured_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "blocks":[
                {"type":"paragraph","text":"Draft wording","bold":true},
                {"type":"paragraph","text":"Remove this sentence"}
            ]
        }),
        &state,
        &request,
    )
    .expect("source DOCX");
    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/source.docx",
            "selection":"Draft wording",
            "replacement":"Approved wording",
            "target_path":"artifacts/replaced.docx",
            "author":"Reviewer"
        }),
        &state,
        &request,
    )
    .expect("tracked replacement");
    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"artifacts/replaced.docx",
            "selection":"Remove this sentence",
            "replacement":"",
            "target_path":"artifacts/revised.docx",
            "author":"Reviewer"
        }),
        &state,
        &request,
    )
    .expect("tracked deletion");

    let revised_before =
        fs::read(root.join("artifacts/revised.docx")).expect("revised source bytes");
    let inspected = inspect_docx(&json!({"path":"artifacts/revised.docx"}), &state, &request)
        .expect("inspect tracked revisions");
    assert_eq!(
        inspected
            .get("selective_revision_resolution_available")
            .and_then(Value::as_bool),
        Some(true)
    );
    let revisions = inspected
        .get("tracked_revisions")
        .and_then(Value::as_array)
        .expect("tracked revision metadata");
    assert_eq!(
        revisions
            .iter()
            .filter_map(|revision| revision.get("revision_id").and_then(Value::as_u64))
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        revisions
            .iter()
            .filter_map(|revision| revision.get("kind").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["deletion", "insertion", "deletion"]
    );
    assert!(revisions.iter().all(|revision| {
        revision.get("author").and_then(Value::as_str) == Some("Reviewer")
            && revision.get("date").and_then(Value::as_str).is_some()
            && revision
                .get("text_preview")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.is_empty())
    }));

    let accepted = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"artifacts/revised.docx",
            "action":"accept",
            "revision_ids":[0,1],
            "target_path":"artifacts/selected-accepted.docx"
        }),
        &state,
        &request,
    )
    .expect("selectively accept replacement revisions");
    assert_eq!(
        accepted.get("resolution_scope").and_then(Value::as_str),
        Some("selected")
    );
    assert_eq!(accepted.get("requested_revision_ids"), Some(&json!([0, 1])));
    assert_eq!(accepted.get("resolved_revision_ids"), Some(&json!([0, 1])));
    assert_eq!(
        accepted
            .get("remaining_tracked_revisions")
            .and_then(Value::as_u64),
        Some(1)
    );
    let accepted_inspection = inspect_docx(
        &json!({"path":"artifacts/selected-accepted.docx"}),
        &state,
        &request,
    )
    .expect("inspect selected acceptance");
    assert_eq!(
        accepted_inspection
            .get("tracked_revisions")
            .and_then(Value::as_array)
            .and_then(|revisions| revisions.first())
            .and_then(|revision| revision.get("revision_id"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert!(accepted_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Approved wording") && !text.contains("Draft wording")));

    let rejected = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"artifacts/selected-accepted.docx",
            "action":"reject",
            "revision_ids":[2],
            "target_path":"artifacts/final.docx"
        }),
        &state,
        &request,
    )
    .expect("selectively reject remaining deletion");
    assert_eq!(rejected.get("resolved_revision_ids"), Some(&json!([2])));
    assert_eq!(
        rejected
            .get("remaining_tracked_revisions")
            .and_then(Value::as_u64),
        Some(0)
    );
    let final_inspection = inspect_docx(&json!({"path":"artifacts/final.docx"}), &state, &request)
        .expect("inspect final DOCX");
    assert!(final_inspection
        .get("tracked_revisions")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty));
    assert!(final_inspection
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Approved wording") && text.contains("Remove this sentence")
        }));
    assert_eq!(
        fs::read(root.join("artifacts/revised.docx")).expect("revised source after resolution"),
        revised_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn selective_docx_revision_resolution_rejects_invalid_missing_and_ambiguous_ids() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({"target_path":"plain.docx","paragraphs":["Original"]}),
        &state,
        &request,
    )
    .expect("plain DOCX");
    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"plain.docx",
            "selection":"Original",
            "replacement":"Replacement",
            "target_path":"revised.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked source");

    for (index, revision_ids, expected) in [
        (0, json!([]), "between 1 and 1000"),
        (1, json!([1, 0]), "strictly increasing"),
        (2, json!([0, 0]), "strictly increasing"),
        (3, json!([99]), "does not exist"),
    ] {
        let target = format!("invalid-{index}.docx");
        let error = docx_edit::resolve_docx_tracked_changes(
            &json!({
                "path":"revised.docx",
                "action":"accept",
                "revision_ids":revision_ids,
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("invalid revision IDs must fail");
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!root.join(format!("invalid-{index}.docx")).exists());
    }

    let duplicate_id_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:ins w:id="5" w:author="A"><w:r><w:t>New</w:t></w:r></w:ins><w:del w:id="5" w:author="B"><w:r><w:delText>Old</w:delText></w:r></w:del></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("duplicate-ids.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            (
                "word/document.xml".to_string(),
                duplicate_id_xml.to_string(),
            ),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ],
        false,
    )
    .expect("duplicate revision ID fixture");
    let duplicate_inspection =
        inspect_docx(&json!({"path":"duplicate-ids.docx"}), &state, &request)
            .expect("inspect duplicate IDs");
    assert_eq!(
        duplicate_inspection
            .get("selective_revision_resolution_available")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(duplicate_inspection
        .get("tracked_revision_inspection_warning")
        .and_then(Value::as_str)
        .is_some_and(|warning| warning.contains("duplicate revision IDs")));
    let ambiguous = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"duplicate-ids.docx",
            "action":"accept",
            "revision_ids":[5],
            "target_path":"ambiguous.docx"
        }),
        &state,
        &request,
    )
    .expect_err("ambiguous revision ID must fail");
    assert!(ambiguous.to_string().contains("ambiguous"));
    assert!(!root.join("ambiguous.docx").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tracked_change_resolution_rejects_unsupported_ambiguous_and_in_place_edits() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({"target_path":"plain.docx","paragraphs":["No revisions"]}),
        &state,
        &request,
    )
    .expect("plain DOCX");
    let none = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"plain.docx",
            "action":"accept",
            "target_path":"none.docx"
        }),
        &state,
        &request,
    )
    .expect_err("missing revisions must fail");
    assert!(none.to_string().contains("no supported tracked"));

    docx_edit::replace_docx_text_tracked(
        &json!({
            "path":"plain.docx",
            "selection":"No revisions",
            "replacement":"One revision",
            "target_path":"revised.docx"
        }),
        &state,
        &request,
    )
    .expect("tracked source");
    let in_place = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"revised.docx",
            "action":"accept",
            "target_path":"revised.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place resolution must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let fixtures = [
        (
            "property-change.docx",
            r#"<w:p><w:r><w:rPr><w:rPrChange w:id="9"><w:rPr/></w:rPrChange></w:rPr><w:t>Base</w:t></w:r><w:ins w:id="1"><w:r><w:t>New</w:t></w:r></w:ins></w:p>"#,
            "unsupported tracked revision markup",
        ),
        (
            "comment-crossing.docx",
            r#"<w:p><w:commentRangeStart w:id="0"/><w:ins w:id="1"><w:r><w:t>New</w:t></w:r></w:ins><w:commentRangeEnd w:id="0"/></w:p>"#,
            "comment range",
        ),
        (
            "nested.docx",
            r#"<w:p><w:ins w:id="1"><w:r><w:t>Outer</w:t></w:r><w:del w:id="2"><w:r><w:delText>Inner</w:delText></w:r></w:del></w:ins></w:p>"#,
            "nested",
        ),
    ];
    for (source, body, expected) in fixtures {
        let document_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr/></w:body></w:document>"
        );
        write_zip(
            root.join(source).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                ("word/document.xml".to_string(), document_xml),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("revision fixture");
        let unsupported_inspection = inspect_docx(&json!({"path":source}), &state, &request)
            .expect("unsupported revision inspection remains readable");
        assert_eq!(
            unsupported_inspection
                .get("selective_revision_resolution_available")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(unsupported_inspection
            .get("tracked_revision_inspection_warning")
            .and_then(Value::as_str)
            .is_some_and(|warning| warning.contains(expected)));
        let target = format!("resolved-{source}");
        let error = docx_edit::resolve_docx_tracked_changes(
            &json!({
                "path":source,
                "action":"reject",
                "revision_ids":[1],
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsupported revision must fail closed");
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!root.join(target).exists());
    }
    let invalid_action = docx_edit::resolve_docx_tracked_changes(
        &json!({
            "path":"revised.docx",
            "action":"merge",
            "target_path":"invalid-action.docx"
        }),
        &state,
        &request,
    )
    .expect_err("invalid action must fail");
    assert!(invalid_action.to_string().contains("accept or reject"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_edits_reject_in_place_and_cross_run_guessing() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("split.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ],
        false,
    )
    .expect("split-run DOCX");

    let in_place = docx_edit::append_docx_content(
        &json!({
            "path":"split.docx",
            "target_path":"split.docx",
            "overwrite":true,
            "blocks":[{"type":"paragraph","text":"No"}]
        }),
        &state,
        &request,
    )
    .expect_err("in-place DOCX edit must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let cross_run = docx_edit::replace_docx_text(
        &json!({
            "path":"split.docx",
            "find":"Hello",
            "replace":"World",
            "target_path":"output.docx"
        }),
        &state,
        &request,
    )
    .expect_err("cross-run replacement must fail closed");
    assert!(cross_run.to_string().contains("individual DOCX text run"));
    assert!(!root.join("output.docx").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn updates_and_inspects_unicode_docx_metadata_while_preserving_unrelated_package_content() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    let core_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>Old title</dc:title><dc:creator>Old author</dc:creator><dc:subject>Old subject</dc:subject><cp:keywords>old</cp:keywords><cp:lastModifiedBy>Existing Editor</cp:lastModifiedBy></cp:coreProperties>"#;
    let root_relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;
    let content_types = docx_content_types().replace(
        "</Types>",
        r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#,
    );
    write_zip(
        root.join("metadata-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), content_types.clone()),
            ("_rels/.rels".to_string(), root_relationships.to_string()),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            ("docProps/core.xml".to_string(), core_xml.to_string()),
            (
                "word/custom.xml".to_string(),
                "<custom>unchanged</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("metadata source DOCX");
    let source = root.join("metadata-source.docx");
    let source_before = fs::read(source.as_path()).expect("metadata source bytes");

    let updated = docx_edit::update_docx_metadata(
        &json!({
            "path":"metadata-source.docx",
            "title":"合同 & 审阅",
            "author":"李雷",
            "keywords":"法律, 已批准",
            "remove_fields":["subject"],
            "target_path":"metadata-updated.docx"
        }),
        &state,
        &request,
    )
    .expect("update DOCX metadata");
    assert_eq!(
        updated.get("operation").and_then(Value::as_str),
        Some("update_metadata")
    );
    assert_eq!(
        updated
            .get("metadata_part_created")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        updated.pointer("/metadata/title").and_then(Value::as_str),
        Some("合同 & 审阅")
    );
    assert!(updated
        .pointer("/metadata/subject")
        .is_some_and(Value::is_null));

    let mut output = ZipArchive::new(
        File::open(root.join("metadata-updated.docx")).expect("updated metadata DOCX"),
    )
    .expect("updated metadata ZIP");
    let updated_core = read_zip_text(&mut output, "docProps/core.xml").expect("updated core XML");
    assert!(updated_core.contains("<dc:title>合同 &amp; 审阅</dc:title>"));
    assert!(updated_core.contains("<dc:creator>李雷</dc:creator>"));
    assert!(updated_core.contains("<cp:keywords>法律, 已批准</cp:keywords>"));
    assert!(!updated_core.contains("<dc:subject>"));
    assert!(updated_core.contains("<cp:lastModifiedBy>Existing Editor</cp:lastModifiedBy>"));
    assert_eq!(
        read_zip_text(&mut output, "_rels/.rels").expect("root relationships"),
        root_relationships
    );
    assert_eq!(
        read_zip_text(&mut output, "[Content_Types].xml").expect("content types"),
        content_types
    );
    assert_eq!(
        read_zip_text(&mut output, "word/document.xml").expect("document XML"),
        document_xml
    );
    assert_eq!(
        read_zip_text(&mut output, "word/custom.xml").expect("custom XML"),
        "<custom>unchanged</custom>"
    );
    drop(output);
    let inspected = inspect_docx(&json!({"path":"metadata-updated.docx"}), &state, &request)
        .expect("inspect updated DOCX metadata");
    assert_eq!(
        inspected.pointer("/metadata/title").and_then(Value::as_str),
        Some("合同 & 审阅")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/author")
            .and_then(Value::as_str),
        Some("李雷")
    );
    assert!(inspected
        .pointer("/metadata/subject")
        .is_some_and(Value::is_null));
    assert_eq!(
        fs::read(source.as_path()).expect("source after metadata update"),
        source_before
    );

    create_docx(
        &json!({"target_path":"without-metadata.docx","paragraphs":["Body"]}),
        &state,
        &request,
    )
    .expect("DOCX without metadata");
    let created = docx_edit::update_docx_metadata(
        &json!({
            "path":"without-metadata.docx",
            "title":"Created metadata",
            "target_path":"with-metadata.docx"
        }),
        &state,
        &request,
    )
    .expect("create missing DOCX metadata");
    assert_eq!(
        created
            .get("metadata_part_created")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut created_archive =
        ZipArchive::new(File::open(root.join("with-metadata.docx")).expect("DOCX with metadata"))
            .expect("created metadata ZIP");
    assert!(read_zip_text(&mut created_archive, "_rels/.rels")
        .expect("created root relationships")
        .contains("metadata/core-properties"));
    assert!(read_zip_text(&mut created_archive, "[Content_Types].xml")
        .expect("created content types")
        .contains("/docProps/core.xml"));
    assert!(read_zip_text(&mut created_archive, "docProps/core.xml")
        .expect("created core properties")
        .contains("<dc:title>Created metadata</dc:title>"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_metadata_update_rejects_noop_overlap_partial_malformed_and_in_place_requests() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    let standard_core = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Existing</dc:title></cp:coreProperties>"#;
    let content_types = docx_content_types().replace(
        "</Types>",
        r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#,
    );
    let standard_relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;
    let write_source = |name: &str, relationships: &str, types: String, core: Option<&str>| {
        let mut entries = vec![
            ("[Content_Types].xml".to_string(), types),
            ("_rels/.rels".to_string(), relationships.to_string()),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
        ];
        if let Some(core) = core {
            entries.push(("docProps/core.xml".to_string(), core.to_string()));
        }
        write_zip(root.join(name).as_path(), entries, false).expect("metadata failure source");
    };
    write_source(
        "standard.docx",
        standard_relationships,
        content_types.clone(),
        Some(standard_core),
    );
    write_source(
        "relation-only.docx",
        standard_relationships,
        docx_content_types(),
        None,
    );
    write_source(
        "part-only.docx",
        office_root_relationships("word/document.xml").as_str(),
        content_types.clone(),
        Some(standard_core),
    );
    let duplicate_core = standard_core.replace(
        "</cp:coreProperties>",
        "<dc:title>Duplicate</dc:title></cp:coreProperties>",
    );
    write_source(
        "duplicate.docx",
        standard_relationships,
        content_types,
        Some(duplicate_core.as_str()),
    );
    let duplicate_content_type = docx_content_types().replace(
        "</Types>",
        r#"<Override PartName="/docProps/core.xml" PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#,
    );
    write_source(
        "duplicate-content-type.docx",
        standard_relationships,
        duplicate_content_type,
        Some(standard_core),
    );
    create_docx(
        &json!({"target_path":"missing.docx","paragraphs":["Body"]}),
        &state,
        &request,
    )
    .expect("missing metadata source");

    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"standard.docx","target_path":"empty.docx"}),
            "requires at least one field",
        ),
        (
            1,
            json!({"path":"standard.docx","title":"Changed","remove_fields":["title"],"target_path":"overlap.docx"}),
            "cannot be set and removed",
        ),
        (
            2,
            json!({"path":"standard.docx","remove_fields":["creator"],"target_path":"unknown.docx"}),
            "title, author, subject, or keywords",
        ),
        (
            3,
            json!({"path":"standard.docx","title":"Existing","target_path":"noop.docx"}),
            "would not change",
        ),
        (
            4,
            json!({"path":"relation-only.docx","title":"Changed","target_path":"relation-output.docx"}),
            "partial core-properties",
        ),
        (
            5,
            json!({"path":"part-only.docx","title":"Changed","target_path":"part-output.docx"}),
            "exactly one standard internal relationship",
        ),
        (
            6,
            json!({"path":"duplicate.docx","title":"Changed","target_path":"duplicate-output.docx"}),
            "appears more than once",
        ),
        (
            7,
            json!({"path":"missing.docx","remove_fields":["title"],"target_path":"missing-output.docx"}),
            "would not change",
        ),
        (
            8,
            json!({"path":"duplicate-content-type.docx","title":"Changed","target_path":"duplicate-content-type-output.docx"}),
            "duplicate PartName",
        ),
    ] {
        let error = docx_edit::update_docx_metadata(&arguments, &state, &request)
            .expect_err("unsafe DOCX metadata update must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let in_place = docx_edit::update_docx_metadata(
        &json!({
            "path":"standard.docx",
            "title":"Changed",
            "target_path":"standard.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place DOCX metadata update must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    for target in [
        "empty.docx",
        "overlap.docx",
        "unknown.docx",
        "noop.docx",
        "relation-output.docx",
        "part-output.docx",
        "duplicate-output.docx",
        "missing-output.docx",
        "duplicate-content-type-output.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_structured_docx_content_before_and_after_unique_top_level_paragraph() {
    let (root, state, request) = test_context();
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Intro</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t xml:space="preserve">Anchor </w:t></w:r><w:r><w:t>text</w:t></w:r></w:p><w:p><w:r><w:t>End</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;
    write_zip(
        root.join("anchor-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.to_string()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>unchanged</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("paragraph anchor source");
    let source = root.join("anchor-source.docx");
    let source_before = fs::read(source.as_path()).expect("paragraph anchor source bytes");

    let after = docx_edit::insert_docx_content_at_paragraph(
        &json!({
            "path":"anchor-source.docx",
            "anchor_text":"Anchor text",
            "position":"after",
            "blocks":[
                {"type":"paragraph","text":"Inserted paragraph","style":"quote","italic":true},
                {"type":"table","header_row":true,"rows":[["Item","Value"],["A","1"]]},
                {"type":"page_break"}
            ],
            "target_path":"inserted-after.docx"
        }),
        &state,
        &request,
    )
    .expect("insert after top-level paragraph");
    assert_eq!(
        after.get("operation").and_then(Value::as_str),
        Some("insert_at_paragraph")
    );
    assert_eq!(
        after.get("anchor_paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(after.get("position").and_then(Value::as_str), Some("after"));
    assert_eq!(after.get("tables").and_then(Value::as_u64), Some(1));
    assert_eq!(after.get("page_breaks").and_then(Value::as_u64), Some(1));

    let mut after_archive =
        ZipArchive::new(File::open(root.join("inserted-after.docx")).expect("inserted-after DOCX"))
            .expect("inserted-after ZIP");
    let after_xml =
        read_zip_text(&mut after_archive, "word/document.xml").expect("inserted-after XML");
    let anchor_index = after_xml.find("Anchor ").expect("anchor text");
    let inserted_index = after_xml
        .find("Inserted paragraph")
        .expect("inserted paragraph");
    let end_index = after_xml.find(">End<").expect("end paragraph");
    assert!(anchor_index < inserted_index && inserted_index < end_index);
    assert!(after_xml.contains("<w:tbl>"));
    assert!(after_xml.contains("<w:br w:type=\"page\"/>"));
    assert!(after_xml.contains("<w:pStyle w:val=\"Heading1\"/>"));
    assert_eq!(
        read_zip_text(&mut after_archive, "word/custom.xml").expect("custom XML"),
        "<custom>unchanged</custom>"
    );

    docx_edit::insert_docx_content_at_paragraph(
        &json!({
            "path":"anchor-source.docx",
            "anchor_text":"Anchor text",
            "position":"before",
            "blocks":[{"type":"paragraph","text":"Before anchor"}],
            "target_path":"inserted-before.docx"
        }),
        &state,
        &request,
    )
    .expect("insert before top-level paragraph");
    let mut before_archive = ZipArchive::new(
        File::open(root.join("inserted-before.docx")).expect("inserted-before DOCX"),
    )
    .expect("inserted-before ZIP");
    let before_xml =
        read_zip_text(&mut before_archive, "word/document.xml").expect("inserted-before XML");
    assert!(
        before_xml.find("Before anchor").expect("before paragraph")
            < before_xml.find("Anchor ").expect("anchor text")
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after paragraph insertions"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_paragraph_anchor_insertion_rejects_duplicate_nested_complex_partial_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("paragraph anchor failure source");
    };
    write_source("valid.docx", "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>");
    write_source(
        "duplicate.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "table.docx",
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    write_source(
        "bookmark.docx",
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"mark\"/><w:r><w:t>Anchor text</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p>",
    );
    write_source(
        "section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "wrapper.docx",
        "<w:sdt><w:sdtContent><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p></w:sdtContent></w:sdt>",
    );

    for (index, source, anchor, expected) in [
        (0, "duplicate.docx", "Anchor text", "exactly one"),
        (1, "table.docx", "Anchor text", "direct top-level child"),
        (2, "bookmark.docx", "Anchor text", "bookmark"),
        (3, "section.docx", "Anchor text", "section properties"),
        (4, "wrapper.docx", "Anchor text", "direct top-level child"),
        (
            5,
            "valid.docx",
            "Anchor",
            "complete visible text of a DOCX paragraph",
        ),
    ] {
        let target = format!("invalid-{index}.docx");
        let error = docx_edit::insert_docx_content_at_paragraph(
            &json!({
                "path":source,
                "anchor_text":anchor,
                "position":"after",
                "blocks":[{"type":"paragraph","text":"Inserted"}],
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsafe paragraph anchor insertion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let invalid_position = docx_edit::insert_docx_content_at_paragraph(
        &json!({
            "path":"valid.docx",
            "anchor_text":"Anchor text",
            "position":"inside",
            "blocks":[{"type":"paragraph","text":"Inserted"}],
            "target_path":"invalid-position.docx"
        }),
        &state,
        &request,
    )
    .expect_err("invalid paragraph insertion position must fail");
    assert!(invalid_position
        .to_string()
        .contains("position must be before or after"));
    let control = docx_edit::insert_docx_content_at_paragraph(
        &json!({
            "path":"valid.docx",
            "anchor_text":"bad\u{0000}anchor",
            "position":"after",
            "blocks":[{"type":"paragraph","text":"Inserted"}],
            "target_path":"control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control character anchor must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::insert_docx_content_at_paragraph(
        &json!({
            "path":"valid.docx",
            "anchor_text":"Anchor text",
            "position":"after",
            "blocks":[{"type":"paragraph","text":"Inserted"}],
            "target_path":"valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place paragraph insertion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    for target in [
        "invalid-0.docx",
        "invalid-1.docx",
        "invalid-2.docx",
        "invalid-3.docx",
        "invalid-4.docx",
        "invalid-5.docx",
        "invalid-position.docx",
        "control.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deletes_unique_top_level_docx_paragraph_without_modifying_source() {
    let (root, state, request) = test_context();
    let anchor_xml = r#"<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t xml:space="preserve">Remove </w:t></w:r><w:r><w:t>this paragraph</w:t></w:r></w:p>"#;
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Keep before</w:t></w:r></w:p>{anchor_xml}<w:p><w:r><w:t>Keep after</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("delete-paragraph-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("paragraph deletion source");
    let source = root.join("delete-paragraph-source.docx");
    let source_before = fs::read(source.as_path()).expect("paragraph deletion source bytes");

    let deleted = docx_edit::delete_docx_paragraph(
        &json!({
            "path":"delete-paragraph-source.docx",
            "anchor_text":"Remove this paragraph",
            "target_path":"deleted-paragraph.docx"
        }),
        &state,
        &request,
    )
    .expect("delete unique top-level paragraph");
    assert_eq!(
        deleted.get("operation").and_then(Value::as_str),
        Some("delete_paragraph")
    );
    assert_eq!(
        deleted.get("anchor_paragraph").and_then(Value::as_u64),
        Some(2)
    );
    let mut archive = ZipArchive::new(
        File::open(root.join("deleted-paragraph.docx")).expect("deleted paragraph DOCX"),
    )
    .expect("deleted paragraph ZIP");
    let output_xml =
        read_zip_text(&mut archive, "word/document.xml").expect("deleted paragraph XML");
    assert_eq!(output_xml, document_xml.replacen(anchor_xml, "", 1));
    assert!(output_xml.contains("Keep before"));
    assert!(output_xml.contains("Keep after"));
    assert!(output_xml.contains("<w:sectPr/>"));
    assert_eq!(
        read_zip_text(&mut archive, "word/custom.xml").expect("preserved custom XML"),
        "<custom>preserved</custom>"
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after paragraph deletion"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_paragraph_deletion_rejects_duplicate_nested_complex_partial_malformed_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("paragraph deletion failure source");
    };
    write_source(
        "delete-valid.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "delete-duplicate.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "delete-table.docx",
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    write_source(
        "delete-bookmark.docx",
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"mark\"/><w:r><w:t>Anchor text</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p>",
    );
    write_source(
        "delete-section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "delete-wrapper.docx",
        "<w:sdt><w:sdtContent><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p></w:sdtContent></w:sdt>",
    );
    write_source(
        "delete-malformed.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r>",
    );

    for (index, source, anchor, expected) in [
        (0, "delete-duplicate.docx", "Anchor text", "exactly one"),
        (
            1,
            "delete-table.docx",
            "Anchor text",
            "direct top-level child",
        ),
        (2, "delete-bookmark.docx", "Anchor text", "bookmark"),
        (
            3,
            "delete-section.docx",
            "Anchor text",
            "section properties",
        ),
        (
            4,
            "delete-wrapper.docx",
            "Anchor text",
            "direct top-level child",
        ),
        (
            5,
            "delete-valid.docx",
            "Anchor",
            "complete visible text of a DOCX paragraph",
        ),
        (
            6,
            "delete-malformed.docx",
            "Anchor text",
            "unclosed element",
        ),
    ] {
        let target = format!("delete-invalid-{index}.docx");
        let error = docx_edit::delete_docx_paragraph(
            &json!({
                "path":source,
                "anchor_text":anchor,
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsafe paragraph deletion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let control = docx_edit::delete_docx_paragraph(
        &json!({
            "path":"delete-valid.docx",
            "anchor_text":"bad\u{0000}anchor",
            "target_path":"delete-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control character paragraph anchor must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::delete_docx_paragraph(
        &json!({
            "path":"delete-valid.docx",
            "anchor_text":"Anchor text",
            "target_path":"delete-valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place paragraph deletion must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    for target in [
        "delete-invalid-0.docx",
        "delete-invalid-1.docx",
        "delete-invalid-2.docx",
        "delete-invalid-3.docx",
        "delete-invalid-4.docx",
        "delete-invalid-5.docx",
        "delete-invalid-6.docx",
        "delete-control.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deletes_indexed_empty_and_repeated_top_level_docx_paragraphs_without_modifying_source() {
    let (root, state, request) = test_context();
    let repeated_first = "<w:p><w:r><w:t>Repeated</w:t></w:r></w:p>";
    let table = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nested table paragraph</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let empty = "<w:p/>";
    let styled_empty =
        "<w:p><w:pPr><w:pStyle w:val=\"Quote\"/><w:jc w:val=\"center\"/></w:pPr></w:p>";
    let repeated_second =
        "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Repeated</w:t></w:r></w:p>";
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{repeated_first}{table}{empty}{styled_empty}{repeated_second}<w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("indexed paragraph source");
    let source_before = fs::read(root.join("source.docx")).expect("source DOCX bytes");

    let inspected = inspect_docx(&json!({"path":"source.docx"}), &state, &request)
        .expect("inspect indexed paragraphs");
    assert_eq!(
        inspected
            .get("top_level_paragraph_count")
            .and_then(Value::as_u64),
        Some(4)
    );
    let paragraphs = inspected
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .expect("top-level paragraph metadata");
    assert_eq!(paragraphs.len(), 4);
    assert_eq!(
        paragraphs[0].get("text").and_then(Value::as_str),
        Some("Repeated")
    );
    assert_eq!(paragraphs[1].get("text").and_then(Value::as_str), Some(""));
    assert_eq!(paragraphs[2].get("text").and_then(Value::as_str), Some(""));
    assert_eq!(
        paragraphs[3].get("text").and_then(Value::as_str),
        Some("Repeated")
    );
    assert!(paragraphs.iter().all(|paragraph| {
        paragraph
            .get("eligible_for_index_deletion")
            .and_then(Value::as_bool)
            == Some(true)
    }));

    let deleted_empty = docx_edit::delete_docx_paragraph_at_index(
        &json!({
            "path":"source.docx",
            "paragraph":2,
            "expected_text":"",
            "target_path":"deleted-empty.docx"
        }),
        &state,
        &request,
    )
    .expect("delete self-closing empty paragraph");
    assert_eq!(
        deleted_empty.get("operation").and_then(Value::as_str),
        Some("delete_paragraph_at_index")
    );
    assert_eq!(
        deleted_empty.get("paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        deleted_empty
            .get("top_level_paragraphs_before")
            .and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        deleted_empty
            .get("top_level_paragraphs_after")
            .and_then(Value::as_u64),
        Some(3)
    );
    let mut empty_archive =
        ZipArchive::new(File::open(root.join("deleted-empty.docx")).expect("empty output"))
            .expect("empty output ZIP");
    assert_eq!(
        read_zip_text(&mut empty_archive, "word/document.xml").expect("empty output XML"),
        document_xml.replacen(empty, "", 1)
    );
    assert_eq!(
        read_zip_text(&mut empty_archive, "word/custom.xml").expect("custom XML"),
        "<custom>preserved</custom>"
    );

    docx_edit::delete_docx_paragraph_at_index(
        &json!({
            "path":"source.docx",
            "paragraph":3,
            "expected_text":"",
            "target_path":"deleted-styled-empty.docx"
        }),
        &state,
        &request,
    )
    .expect("delete styled empty paragraph");
    let mut styled_archive = ZipArchive::new(
        File::open(root.join("deleted-styled-empty.docx")).expect("styled empty output"),
    )
    .expect("styled empty output ZIP");
    assert_eq!(
        read_zip_text(&mut styled_archive, "word/document.xml").expect("styled empty output XML"),
        document_xml.replacen(styled_empty, "", 1)
    );

    docx_edit::delete_docx_paragraph_at_index(
        &json!({
            "path":"source.docx",
            "paragraph":4,
            "expected_text":"Repeated",
            "target_path":"deleted-second-repeated.docx"
        }),
        &state,
        &request,
    )
    .expect("delete specifically indexed repeated paragraph");
    let mut repeated_archive = ZipArchive::new(
        File::open(root.join("deleted-second-repeated.docx")).expect("repeated output"),
    )
    .expect("repeated output ZIP");
    let repeated_xml =
        read_zip_text(&mut repeated_archive, "word/document.xml").expect("repeated output XML");
    assert_eq!(repeated_xml, document_xml.replacen(repeated_second, "", 1));
    assert!(repeated_xml.contains(repeated_first));
    assert!(repeated_xml.contains("Nested table paragraph"));
    assert_eq!(
        fs::read(root.join("source.docx")).expect("source after indexed deletions"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn indexed_docx_paragraph_deletion_rejects_mismatch_ranges_complex_invalid_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("indexed paragraph rejection source");
    };
    write_source(
        "valid.docx",
        "<w:p><w:r><w:t>First</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>Last</w:t></w:r></w:p>",
    );
    write_source(
        "range.docx",
        "<w:bookmarkStart w:id=\"1\" w:name=\"range\"/><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:t>Last</w:t></w:r></w:p><w:bookmarkEnd w:id=\"1\"/>",
    );
    write_source(
        "hyperlink.docx",
        "<w:p><w:hyperlink w:anchor=\"target\"><w:r><w:t>Linked</w:t></w:r></w:hyperlink></w:p>",
    );
    write_source(
        "section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Section</w:t></w:r></w:p>",
    );
    write_source(
        "unsupported-empty.docx",
        "<w:p><w:altChunk w:id=\"rId1\"/></w:p>",
    );
    write_source("malformed.docx", "<w:p><w:r><w:t>Broken</w:t></w:r>");
    let source_before = fs::read(root.join("valid.docx")).expect("valid source bytes");

    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"valid.docx","paragraph":1,"expected_text":"Wrong","target_path":"mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            1,
            json!({"path":"valid.docx","paragraph":0,"expected_text":"First","target_path":"zero.docx"}),
            "paragraph must be an integer between 1 and 2000",
        ),
        (
            2,
            json!({"path":"valid.docx","paragraph":4,"expected_text":"Last","target_path":"outside.docx"}),
            "paragraph index 4",
        ),
        (
            3,
            json!({"path":"valid.docx","paragraph":1,"expected_text":"First","target_path":"valid.docx","overwrite":true}),
            "distinct target_path",
        ),
        (
            4,
            json!({"path":"range.docx","paragraph":1,"expected_text":"First","target_path":"range-output.docx"}),
            "document range markup",
        ),
        (
            5,
            json!({"path":"hyperlink.docx","paragraph":1,"expected_text":"Linked","target_path":"hyperlink-output.docx"}),
            "hyperlink",
        ),
        (
            6,
            json!({"path":"section.docx","paragraph":1,"expected_text":"Section","target_path":"section-output.docx"}),
            "section properties",
        ),
        (
            7,
            json!({"path":"unsupported-empty.docx","paragraph":1,"expected_text":"","target_path":"unsupported-empty-output.docx"}),
            "wrapper element",
        ),
        (
            8,
            json!({"path":"malformed.docx","paragraph":1,"expected_text":"Broken","target_path":"malformed-output.docx"}),
            "mismatched element boundaries",
        ),
    ] {
        let error = docx_edit::delete_docx_paragraph_at_index(&arguments, &state, &request)
            .expect_err("unsafe indexed paragraph deletion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }
    let control = docx_edit::delete_docx_paragraph_at_index(
        &json!({
            "path":"valid.docx",
            "paragraph":1,
            "expected_text":"bad\u{0000}text",
            "target_path":"control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control expected text must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    assert_eq!(
        fs::read(root.join("valid.docx")).expect("valid source after rejections"),
        source_before
    );
    for target in [
        "mismatch.docx",
        "zero.docx",
        "outside.docx",
        "range-output.docx",
        "hyperlink-output.docx",
        "section-output.docx",
        "unsupported-empty-output.docx",
        "malformed-output.docx",
        "control.docx",
    ] {
        assert!(!root.join(target).exists(), "unexpected output {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inserts_content_at_indexed_empty_and_repeated_top_level_docx_paragraphs_without_modifying_source(
) {
    let (root, state, request) = test_context();
    let repeated_first = "<w:p><w:r><w:t>Repeated</w:t></w:r></w:p>";
    let nested_table = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nested table paragraph</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let empty = "<w:p/>";
    let repeated_second = "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Repeated</w:t></w:r></w:p>";
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{repeated_first}{nested_table}{empty}{repeated_second}<w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("indexed-insert-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("indexed insertion source");
    let source_before = fs::read(root.join("indexed-insert-source.docx")).expect("source bytes");

    let inspected = inspect_docx(
        &json!({"path":"indexed-insert-source.docx"}),
        &state,
        &request,
    )
    .expect("inspect indexed insertion anchors");
    assert_eq!(
        inspected
            .get("top_level_paragraph_count")
            .and_then(Value::as_u64),
        Some(3)
    );
    let paragraphs = inspected
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .expect("top-level paragraphs");
    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[1].get("text").and_then(Value::as_str), Some(""));
    assert!(paragraphs.iter().all(|paragraph| {
        paragraph
            .get("eligible_for_index_insertion")
            .and_then(Value::as_bool)
            == Some(true)
    }));

    let inserted_before = docx_edit::insert_docx_content_at_paragraph_index(
        &json!({
            "path":"indexed-insert-source.docx",
            "paragraph":2,
            "expected_text":"",
            "position":"before",
            "blocks":[{"type":"paragraph","text":"Before empty"}],
            "target_path":"inserted-before-empty.docx"
        }),
        &state,
        &request,
    )
    .expect("insert before indexed empty paragraph");
    assert_eq!(
        inserted_before.get("operation").and_then(Value::as_str),
        Some("insert_at_paragraph_index")
    );
    assert_eq!(
        inserted_before.get("paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        inserted_before
            .get("top_level_paragraphs_before")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        inserted_before
            .get("top_level_paragraphs_after")
            .and_then(Value::as_u64),
        Some(4)
    );
    let generated_paragraph = "<w:p><w:pPr><w:pStyle w:val=\"Normal\"/><w:jc w:val=\"left\"/><w:spacing w:after=\"160\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr><w:t xml:space=\"preserve\">Before empty</w:t></w:r></w:p>";
    let mut before_archive = ZipArchive::new(
        File::open(root.join("inserted-before-empty.docx")).expect("before output"),
    )
    .expect("before output ZIP");
    assert_eq!(
        read_zip_text(&mut before_archive, "word/document.xml").expect("before XML"),
        document_xml.replacen(empty, format!("{generated_paragraph}{empty}").as_str(), 1)
    );
    assert_eq!(
        read_zip_text(&mut before_archive, "word/custom.xml").expect("custom XML"),
        "<custom>preserved</custom>"
    );

    docx_edit::insert_docx_content_at_paragraph_index(
        &json!({
            "path":"indexed-insert-source.docx",
            "paragraph":3,
            "expected_text":"Repeated",
            "position":"after",
            "blocks":[{"type":"page_break"}],
            "target_path":"inserted-after-repeated.docx"
        }),
        &state,
        &request,
    )
    .expect("insert after specifically indexed repeated paragraph");
    let page_break = "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";
    let mut repeated_archive = ZipArchive::new(
        File::open(root.join("inserted-after-repeated.docx")).expect("repeated output"),
    )
    .expect("repeated output ZIP");
    let repeated_output =
        read_zip_text(&mut repeated_archive, "word/document.xml").expect("repeated XML");
    assert_eq!(
        repeated_output,
        document_xml.replacen(
            repeated_second,
            format!("{repeated_second}{page_break}").as_str(),
            1,
        )
    );
    assert!(repeated_output.contains(repeated_first));
    assert!(repeated_output.contains("Nested table paragraph"));
    assert_eq!(
        fs::read(root.join("indexed-insert-source.docx")).expect("source after insertions"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn indexed_docx_paragraph_insertion_rejects_mismatch_ranges_complex_invalid_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("indexed insertion rejection source");
    };
    write_source(
        "insert-valid.docx",
        "<w:p><w:r><w:t>First</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>Last</w:t></w:r></w:p>",
    );
    write_source(
        "insert-range.docx",
        "<w:bookmarkStart w:id=\"1\" w:name=\"range\"/><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:r><w:t>Last</w:t></w:r></w:p><w:bookmarkEnd w:id=\"1\"/>",
    );
    write_source(
        "insert-hyperlink.docx",
        "<w:p><w:hyperlink w:anchor=\"target\"><w:r><w:t>Linked</w:t></w:r></w:hyperlink></w:p>",
    );
    write_source(
        "insert-section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Section</w:t></w:r></w:p>",
    );
    write_source(
        "insert-wrapper.docx",
        "<w:p><w:altChunk w:id=\"rId1\"/></w:p>",
    );
    write_source("insert-malformed.docx", "<w:p><w:r><w:t>Broken</w:t></w:r>");
    let source_before = fs::read(root.join("insert-valid.docx")).expect("valid source bytes");

    let range_inspection = inspect_docx(&json!({"path":"insert-range.docx"}), &state, &request)
        .expect("inspect range-marked document");
    assert!(range_inspection
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .is_some_and(|paragraphs| paragraphs.iter().all(|paragraph| {
            paragraph
                .get("eligible_for_index_insertion")
                .and_then(Value::as_bool)
                == Some(false)
                && paragraph
                    .get("eligible_for_index_deletion")
                    .and_then(Value::as_bool)
                    == Some(false)
        })));

    let blocks = json!([{"type":"paragraph","text":"Inserted"}]);
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"insert-valid.docx","paragraph":1,"expected_text":"Wrong","position":"before","blocks":blocks,"target_path":"insert-mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            1,
            json!({"path":"insert-valid.docx","paragraph":0,"expected_text":"First","position":"before","blocks":blocks,"target_path":"insert-zero.docx"}),
            "paragraph must be an integer between 1 and 2000",
        ),
        (
            2,
            json!({"path":"insert-valid.docx","paragraph":4,"expected_text":"Last","position":"after","blocks":blocks,"target_path":"insert-outside.docx"}),
            "paragraph index 4",
        ),
        (
            3,
            json!({"path":"insert-valid.docx","paragraph":1,"expected_text":"First","position":"middle","blocks":blocks,"target_path":"insert-position.docx"}),
            "position must be before or after",
        ),
        (
            4,
            json!({"path":"insert-valid.docx","paragraph":1,"expected_text":"First","position":"before","blocks":blocks,"target_path":"insert-valid.docx","overwrite":true}),
            "distinct target_path",
        ),
        (
            5,
            json!({"path":"insert-range.docx","paragraph":1,"expected_text":"First","position":"before","blocks":blocks,"target_path":"insert-range-output.docx"}),
            "document range markup",
        ),
        (
            6,
            json!({"path":"insert-hyperlink.docx","paragraph":1,"expected_text":"Linked","position":"after","blocks":blocks,"target_path":"insert-hyperlink-output.docx"}),
            "hyperlink",
        ),
        (
            7,
            json!({"path":"insert-section.docx","paragraph":1,"expected_text":"Section","position":"before","blocks":blocks,"target_path":"insert-section-output.docx"}),
            "section properties",
        ),
        (
            8,
            json!({"path":"insert-wrapper.docx","paragraph":1,"expected_text":"","position":"after","blocks":blocks,"target_path":"insert-wrapper-output.docx"}),
            "wrapper element",
        ),
        (
            9,
            json!({"path":"insert-malformed.docx","paragraph":1,"expected_text":"Broken","position":"before","blocks":blocks,"target_path":"insert-malformed-output.docx"}),
            "mismatched element boundaries",
        ),
    ] {
        let error = docx_edit::insert_docx_content_at_paragraph_index(&arguments, &state, &request)
            .expect_err("unsafe indexed paragraph insertion must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }
    let control = docx_edit::insert_docx_content_at_paragraph_index(
        &json!({
            "path":"insert-valid.docx",
            "paragraph":1,
            "expected_text":"bad\u{0000}text",
            "position":"before",
            "blocks":[{"type":"paragraph","text":"Inserted"}],
            "target_path":"insert-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control expected text must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    assert_eq!(
        fs::read(root.join("insert-valid.docx")).expect("valid source after rejections"),
        source_before
    );
    for target in [
        "insert-mismatch.docx",
        "insert-zero.docx",
        "insert-outside.docx",
        "insert-position.docx",
        "insert-range-output.docx",
        "insert-hyperlink-output.docx",
        "insert-section-output.docx",
        "insert-wrapper-output.docx",
        "insert-malformed-output.docx",
        "insert-control.docx",
    ] {
        assert!(!root.join(target).exists(), "unexpected output {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn moves_unique_top_level_docx_paragraph_before_and_after_reference_without_modifying_source() {
    let (root, state, request) = test_context();
    let intro = "<w:p><w:r><w:t>Intro</w:t></w:r></w:p>";
    let moving = r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t xml:space="preserve">Move </w:t></w:r><w:r><w:t>me</w:t></w:r></w:p>"#;
    let middle = "<w:p><w:r><w:t>Middle</w:t></w:r></w:p>";
    let reference = "<w:p><w:r><w:t>Reference</w:t></w:r></w:p>";
    let end = "<w:p><w:r><w:t>End</w:t></w:r></w:p>";
    let wrap = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        )
    };
    let document_xml = wrap(format!("{intro}{moving}{middle}{reference}{end}").as_str());
    write_zip(
        root.join("move-paragraph-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>move-preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("paragraph move source");
    let source = root.join("move-paragraph-source.docx");
    let source_before = fs::read(source.as_path()).expect("paragraph move source bytes");

    let moved_after = docx_edit::move_docx_paragraph(
        &json!({
            "path":"move-paragraph-source.docx",
            "anchor_text":"Move me",
            "reference_text":"Reference",
            "position":"after",
            "target_path":"moved-after.docx"
        }),
        &state,
        &request,
    )
    .expect("move earlier paragraph after later reference");
    assert_eq!(
        moved_after.get("operation").and_then(Value::as_str),
        Some("move_paragraph")
    );
    assert_eq!(
        moved_after.get("anchor_paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        moved_after
            .get("reference_paragraph")
            .and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        moved_after.get("position").and_then(Value::as_str),
        Some("after")
    );
    let mut after_archive =
        ZipArchive::new(File::open(root.join("moved-after.docx")).expect("moved-after DOCX"))
            .expect("moved-after ZIP");
    assert_eq!(
        read_zip_text(&mut after_archive, "word/document.xml").expect("moved-after XML"),
        wrap(format!("{intro}{middle}{reference}{moving}{end}").as_str())
    );
    assert_eq!(
        read_zip_text(&mut after_archive, "word/custom.xml").expect("move custom XML"),
        "<custom>move-preserved</custom>"
    );

    let moved_before = docx_edit::move_docx_paragraph(
        &json!({
            "path":"move-paragraph-source.docx",
            "anchor_text":"End",
            "reference_text":"Intro",
            "position":"before",
            "target_path":"moved-before.docx"
        }),
        &state,
        &request,
    )
    .expect("move later paragraph before earlier reference");
    assert_eq!(
        moved_before.get("anchor_paragraph").and_then(Value::as_u64),
        Some(5)
    );
    assert_eq!(
        moved_before
            .get("reference_paragraph")
            .and_then(Value::as_u64),
        Some(1)
    );
    let mut before_archive =
        ZipArchive::new(File::open(root.join("moved-before.docx")).expect("moved-before DOCX"))
            .expect("moved-before ZIP");
    assert_eq!(
        read_zip_text(&mut before_archive, "word/document.xml").expect("moved-before XML"),
        wrap(format!("{end}{intro}{moving}{middle}{reference}").as_str())
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after paragraph moves"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_paragraph_move_rejects_ambiguous_nested_ranges_noop_invalid_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("paragraph move failure source");
    };
    write_source(
        "move-valid.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p><w:p><w:r><w:t>C</w:t></w:r></w:p>",
    );
    write_source(
        "move-duplicate-anchor.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );
    write_source(
        "move-duplicate-reference.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );
    write_source(
        "move-wrapper-reference.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:sdt><w:sdtContent><w:p><w:r><w:t>B</w:t></w:r></w:p></w:sdtContent></w:sdt>",
    );
    write_source(
        "move-range.docx",
        "<w:p><w:commentRangeStart w:id=\"1\"/><w:r><w:t>Other</w:t></w:r><w:commentRangeEnd w:id=\"1\"/></w:p><w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );

    for (index, source, anchor, reference_text, position, expected) in [
        (
            0,
            "move-valid.docx",
            "A",
            "A",
            "after",
            "distinct paragraphs",
        ),
        (
            1,
            "move-duplicate-anchor.docx",
            "A",
            "B",
            "after",
            "anchor_text must match exactly one",
        ),
        (
            2,
            "move-duplicate-reference.docx",
            "A",
            "B",
            "after",
            "reference_text must match exactly one",
        ),
        (
            3,
            "move-wrapper-reference.docx",
            "A",
            "B",
            "after",
            "reference_text is not an eligible top-level",
        ),
        (4, "move-range.docx", "A", "B", "after", "range markup"),
        (
            5,
            "move-valid.docx",
            "A",
            "B",
            "before",
            "already in the requested position",
        ),
        (
            6,
            "move-valid.docx",
            "A",
            "C",
            "inside",
            "position must be before or after",
        ),
    ] {
        let target = format!("move-invalid-{index}.docx");
        let error = docx_edit::move_docx_paragraph(
            &json!({
                "path":source,
                "anchor_text":anchor,
                "reference_text":reference_text,
                "position":position,
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsafe paragraph move must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let control = docx_edit::move_docx_paragraph(
        &json!({
            "path":"move-valid.docx",
            "anchor_text":"A",
            "reference_text":"bad\u{0000}reference",
            "position":"after",
            "target_path":"move-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control character move reference must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::move_docx_paragraph(
        &json!({
            "path":"move-valid.docx",
            "anchor_text":"C",
            "reference_text":"A",
            "position":"before",
            "target_path":"move-valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place paragraph move must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    for target in [
        "move-invalid-0.docx",
        "move-invalid-1.docx",
        "move-invalid-2.docx",
        "move-invalid-3.docx",
        "move-invalid-4.docx",
        "move-invalid-5.docx",
        "move-invalid-6.docx",
        "move-control.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn moves_indexed_empty_and_repeated_top_level_docx_paragraphs_without_modifying_source() {
    let (root, state, request) = test_context();
    let repeated_first = r#"<w:p><w:r><w:t>Repeated</w:t></w:r></w:p>"#;
    let table = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nested table paragraph</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let empty = r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr></w:p>"#;
    let repeated_second =
        r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t>Repeated</w:t></w:r></w:p>"#;
    let end = r#"<w:p><w:r><w:t>End</w:t></w:r></w:p>"#;
    let wrap = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        )
    };
    let document_xml =
        wrap(format!("{repeated_first}{table}{empty}{repeated_second}{end}").as_str());
    write_zip(
        root.join("indexed-move-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>indexed-move-preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("indexed paragraph move source");
    let source_before = fs::read(root.join("indexed-move-source.docx")).expect("source bytes");

    let inspected = inspect_docx(
        &json!({"path":"indexed-move-source.docx"}),
        &state,
        &request,
    )
    .expect("inspect indexed move paragraphs");
    let paragraphs = inspected
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .expect("top-level paragraphs");
    assert_eq!(paragraphs.len(), 4);
    assert_eq!(paragraphs[1].get("text").and_then(Value::as_str), Some(""));
    assert!(paragraphs.iter().all(|paragraph| {
        paragraph
            .get("eligible_for_index_movement")
            .and_then(Value::as_bool)
            == Some(true)
    }));

    let moved_empty = docx_edit::move_docx_paragraph_at_index(
        &json!({
            "path":"indexed-move-source.docx",
            "paragraph":2,
            "expected_text":"",
            "reference_paragraph":4,
            "reference_expected_text":"End",
            "position":"after",
            "target_path":"indexed-empty-moved.docx"
        }),
        &state,
        &request,
    )
    .expect("move indexed empty paragraph after later reference");
    assert_eq!(
        moved_empty.get("operation").and_then(Value::as_str),
        Some("move_paragraph_at_index")
    );
    assert_eq!(
        moved_empty.get("paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        moved_empty
            .get("reference_paragraph")
            .and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        moved_empty.get("moved_paragraph").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        moved_empty
            .get("top_level_paragraphs")
            .and_then(Value::as_u64),
        Some(4)
    );
    let mut empty_archive = ZipArchive::new(
        File::open(root.join("indexed-empty-moved.docx")).expect("empty move output"),
    )
    .expect("empty move ZIP");
    assert_eq!(
        read_zip_text(&mut empty_archive, "word/document.xml").expect("empty move XML"),
        wrap(format!("{repeated_first}{table}{repeated_second}{end}{empty}").as_str())
    );
    assert_eq!(
        read_zip_text(&mut empty_archive, "word/custom.xml").expect("custom XML"),
        "<custom>indexed-move-preserved</custom>"
    );

    let moved_repeated = docx_edit::move_docx_paragraph_at_index(
        &json!({
            "path":"indexed-move-source.docx",
            "paragraph":3,
            "expected_text":"Repeated",
            "reference_paragraph":1,
            "reference_expected_text":"Repeated",
            "position":"before",
            "target_path":"indexed-repeated-moved.docx"
        }),
        &state,
        &request,
    )
    .expect("move one indexed repeated paragraph before another");
    assert_eq!(
        moved_repeated
            .get("moved_paragraph")
            .and_then(Value::as_u64),
        Some(1)
    );
    let mut repeated_archive = ZipArchive::new(
        File::open(root.join("indexed-repeated-moved.docx")).expect("repeated move output"),
    )
    .expect("repeated move ZIP");
    assert_eq!(
        read_zip_text(&mut repeated_archive, "word/document.xml").expect("repeated move XML"),
        wrap(format!("{repeated_second}{repeated_first}{table}{empty}{end}").as_str())
    );
    assert_eq!(
        fs::read(root.join("indexed-move-source.docx")).expect("source after indexed moves"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn indexed_docx_paragraph_movement_rejects_mismatch_ranges_complex_noop_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("indexed movement rejection source");
    };
    write_source(
        "move-index-valid.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p><w:p><w:r><w:t>C</w:t></w:r></w:p>",
    );
    write_source(
        "move-index-range.docx",
        "<w:bookmarkStart w:id=\"1\" w:name=\"range\"/><w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p><w:bookmarkEnd w:id=\"1\"/>",
    );
    write_source(
        "move-index-hyperlink.docx",
        "<w:p><w:hyperlink w:anchor=\"target\"><w:r><w:t>Linked</w:t></w:r></w:hyperlink></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );
    write_source(
        "move-index-section.docx",
        "<w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Section</w:t></w:r></w:p>",
    );
    write_source(
        "move-index-wrapper.docx",
        "<w:p><w:altChunk w:id=\"rId1\"/></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );
    write_source(
        "move-index-malformed.docx",
        "<w:p><w:r><w:t>A</w:t></w:r><w:p><w:r><w:t>B</w:t></w:r></w:p>",
    );
    let source_before = fs::read(root.join("move-index-valid.docx")).expect("valid source");

    let range_inspection = inspect_docx(&json!({"path":"move-index-range.docx"}), &state, &request)
        .expect("inspect range-marked movement document");
    assert!(range_inspection
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .is_some_and(|paragraphs| paragraphs.iter().all(|paragraph| {
            [
                "eligible_for_index_insertion",
                "eligible_for_index_deletion",
                "eligible_for_index_movement",
                "eligible_for_index_replacement",
            ]
            .iter()
            .all(|field| paragraph.get(*field).and_then(Value::as_bool) == Some(false))
        })));

    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"move-index-valid.docx","paragraph":1,"expected_text":"A","reference_paragraph":1,"reference_expected_text":"A","position":"after","target_path":"move-index-same.docx"}),
            "must select distinct paragraphs",
        ),
        (
            1,
            json!({"path":"move-index-valid.docx","paragraph":0,"expected_text":"A","reference_paragraph":2,"reference_expected_text":"B","position":"after","target_path":"move-index-zero.docx"}),
            "paragraph must be an integer between 1 and 2000",
        ),
        (
            2,
            json!({"path":"move-index-valid.docx","paragraph":1,"expected_text":"A","reference_paragraph":4,"reference_expected_text":"B","position":"after","target_path":"move-index-reference-outside.docx"}),
            "reference_paragraph index 4",
        ),
        (
            3,
            json!({"path":"move-index-valid.docx","paragraph":3,"expected_text":"Wrong","reference_paragraph":1,"reference_expected_text":"A","position":"before","target_path":"move-index-source-mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            4,
            json!({"path":"move-index-valid.docx","paragraph":3,"expected_text":"C","reference_paragraph":1,"reference_expected_text":"Wrong","position":"before","target_path":"move-index-reference-mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            5,
            json!({"path":"move-index-valid.docx","paragraph":3,"expected_text":"C","reference_paragraph":1,"reference_expected_text":"A","position":"inside","target_path":"move-index-position.docx"}),
            "position must be before or after",
        ),
        (
            6,
            json!({"path":"move-index-valid.docx","paragraph":1,"expected_text":"A","reference_paragraph":2,"reference_expected_text":"B","position":"before","target_path":"move-index-noop.docx"}),
            "already in the requested position",
        ),
        (
            7,
            json!({"path":"move-index-range.docx","paragraph":1,"expected_text":"A","reference_paragraph":2,"reference_expected_text":"B","position":"after","target_path":"move-index-range-output.docx"}),
            "document range markup",
        ),
        (
            8,
            json!({"path":"move-index-hyperlink.docx","paragraph":1,"expected_text":"Linked","reference_paragraph":2,"reference_expected_text":"B","position":"after","target_path":"move-index-hyperlink-output.docx"}),
            "hyperlink",
        ),
        (
            9,
            json!({"path":"move-index-section.docx","paragraph":1,"expected_text":"A","reference_paragraph":2,"reference_expected_text":"Section","position":"after","target_path":"move-index-section-output.docx"}),
            "section properties",
        ),
        (
            10,
            json!({"path":"move-index-wrapper.docx","paragraph":1,"expected_text":"","reference_paragraph":2,"reference_expected_text":"B","position":"after","target_path":"move-index-wrapper-output.docx"}),
            "wrapper element",
        ),
        (
            11,
            json!({"path":"move-index-malformed.docx","paragraph":1,"expected_text":"A","reference_paragraph":2,"reference_expected_text":"B","position":"after","target_path":"move-index-malformed-output.docx"}),
            "mismatched element boundaries",
        ),
    ] {
        let error = docx_edit::move_docx_paragraph_at_index(&arguments, &state, &request)
            .expect_err("unsafe indexed paragraph movement must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let control = docx_edit::move_docx_paragraph_at_index(
        &json!({
            "path":"move-index-valid.docx",
            "paragraph":3,
            "expected_text":"C",
            "reference_paragraph":1,
            "reference_expected_text":"bad\u{0000}reference",
            "position":"before",
            "target_path":"move-index-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control reference expected text must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::move_docx_paragraph_at_index(
        &json!({
            "path":"move-index-valid.docx",
            "paragraph":3,
            "expected_text":"C",
            "reference_paragraph":1,
            "reference_expected_text":"A",
            "position":"before",
            "target_path":"move-index-valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place indexed paragraph movement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("move-index-valid.docx")).expect("valid source after rejections"),
        source_before
    );
    for target in [
        "move-index-same.docx",
        "move-index-zero.docx",
        "move-index-reference-outside.docx",
        "move-index-source-mismatch.docx",
        "move-index-reference-mismatch.docx",
        "move-index-position.docx",
        "move-index-noop.docx",
        "move-index-range-output.docx",
        "move-index-hyperlink-output.docx",
        "move-index-section-output.docx",
        "move-index-wrapper-output.docx",
        "move-index-malformed-output.docx",
        "move-index-control.docx",
    ] {
        assert!(!root.join(target).exists(), "unexpected output {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_unique_top_level_docx_paragraph_with_structured_content() {
    let (root, state, request) = test_context();
    let anchor_xml = r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t xml:space="preserve">Replace </w:t></w:r><w:r><w:t>this</w:t></w:r></w:p>"#;
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Keep before</w:t></w:r></w:p>{anchor_xml}<w:p><w:r><w:t>Keep after</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("replace-paragraph-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>replacement-preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("paragraph replacement source");
    let source = root.join("replace-paragraph-source.docx");
    let source_before = fs::read(source.as_path()).expect("paragraph replacement source bytes");

    let replaced = docx_edit::replace_docx_paragraph_with_content(
        &json!({
            "path":"replace-paragraph-source.docx",
            "anchor_text":"Replace this",
            "blocks":[
                {"type":"paragraph","text":"Replacement heading","style":"heading2"},
                {"type":"table","header_row":true,"rows":[["Name","Value"],["A","1"]]},
                {"type":"page_break"}
            ],
            "target_path":"replaced-paragraph.docx"
        }),
        &state,
        &request,
    )
    .expect("replace paragraph with structured content");
    assert_eq!(
        replaced.get("operation").and_then(Value::as_str),
        Some("replace_paragraph_with_content")
    );
    assert_eq!(
        replaced.get("anchor_paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(replaced.get("tables").and_then(Value::as_u64), Some(1));
    assert_eq!(replaced.get("page_breaks").and_then(Value::as_u64), Some(1));
    let mut archive = ZipArchive::new(
        File::open(root.join("replaced-paragraph.docx")).expect("replaced paragraph DOCX"),
    )
    .expect("replaced paragraph ZIP");
    let output_xml =
        read_zip_text(&mut archive, "word/document.xml").expect("replaced paragraph XML");
    let before_index = output_xml.find("Keep before").expect("before paragraph");
    let heading_index = output_xml
        .find("Replacement heading")
        .expect("replacement heading");
    let table_index = output_xml.find("<w:tbl>").expect("replacement table");
    let page_break_index = output_xml
        .find("<w:br w:type=\"page\"/>")
        .expect("replacement page break");
    let after_index = output_xml.find("Keep after").expect("after paragraph");
    assert!(
        before_index < heading_index
            && heading_index < table_index
            && table_index < page_break_index
            && page_break_index < after_index
    );
    assert!(!output_xml.contains("Replace this"));
    assert!(!output_xml.contains("Replace "));
    assert!(output_xml.contains("<w:pStyle w:val=\"Heading2\"/>"));
    assert!(output_xml.contains("<w:sectPr/>"));
    assert_eq!(
        read_zip_text(&mut archive, "word/custom.xml").expect("replacement custom XML"),
        "<custom>replacement-preserved</custom>"
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after paragraph replacement"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_paragraph_structured_replacement_rejects_unsafe_noop_and_in_place_requests() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("paragraph structured replacement failure source");
    };
    write_source(
        "replace-valid.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "replace-duplicate.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r></w:p><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "replace-table.docx",
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    write_source(
        "replace-section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "replace-range.docx",
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"range\"/><w:r><w:t>Other</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p><w:p><w:r><w:t>Anchor text</w:t></w:r></w:p>",
    );
    write_source(
        "replace-malformed.docx",
        "<w:p><w:r><w:t>Anchor text</w:t></w:r>",
    );
    let identical_paragraph = r#"<w:p><w:pPr><w:pStyle w:val="Normal"/><w:jc w:val="left"/><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr><w:t xml:space="preserve">Anchor</w:t></w:r></w:p>"#;
    write_source("replace-noop.docx", identical_paragraph);

    for (index, source, anchor, blocks, expected) in [
        (
            0,
            "replace-duplicate.docx",
            "Anchor text",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "exactly one",
        ),
        (
            1,
            "replace-table.docx",
            "Anchor text",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "direct top-level child",
        ),
        (
            2,
            "replace-section.docx",
            "Anchor text",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "section properties",
        ),
        (
            3,
            "replace-range.docx",
            "Anchor text",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "range markup",
        ),
        (
            4,
            "replace-valid.docx",
            "Anchor",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "complete visible text of a DOCX paragraph",
        ),
        (
            5,
            "replace-malformed.docx",
            "Anchor text",
            json!([{"type":"paragraph","text":"Replacement"}]),
            "unclosed element",
        ),
        (
            6,
            "replace-noop.docx",
            "Anchor",
            json!([{"type":"paragraph","text":"Anchor"}]),
            "identical to the selected DOCX paragraph",
        ),
    ] {
        let target = format!("replace-structured-invalid-{index}.docx");
        let error = docx_edit::replace_docx_paragraph_with_content(
            &json!({
                "path":source,
                "anchor_text":anchor,
                "blocks":blocks,
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsafe paragraph structured replacement must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let control = docx_edit::replace_docx_paragraph_with_content(
        &json!({
            "path":"replace-valid.docx",
            "anchor_text":"bad\u{0000}anchor",
            "blocks":[{"type":"paragraph","text":"Replacement"}],
            "target_path":"replace-structured-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control character structured replacement anchor must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::replace_docx_paragraph_with_content(
        &json!({
            "path":"replace-valid.docx",
            "anchor_text":"Anchor text",
            "blocks":[{"type":"paragraph","text":"Replacement"}],
            "target_path":"replace-valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place paragraph structured replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    for target in [
        "replace-structured-invalid-0.docx",
        "replace-structured-invalid-1.docx",
        "replace-structured-invalid-2.docx",
        "replace-structured-invalid-3.docx",
        "replace-structured-invalid-4.docx",
        "replace-structured-invalid-5.docx",
        "replace-structured-invalid-6.docx",
        "replace-structured-control.docx",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_indexed_top_level_docx_paragraph_with_structured_content() {
    let (root, state, request) = test_context();
    let repeated_first = r#"<w:p><w:r><w:t>Repeated</w:t></w:r></w:p>"#;
    let empty = r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr></w:p>"#;
    let repeated_second =
        r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t>Repeated</w:t></w:r></w:p>"#;
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{repeated_first}<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nested table paragraph</w:t></w:r></w:p></w:tc></w:tr></w:tbl>{empty}{repeated_second}<w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("indexed-replace-source.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>indexed-replacement-preserved</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("indexed replacement source");
    let source_before = fs::read(root.join("indexed-replace-source.docx")).expect("source bytes");

    let inspected = inspect_docx(
        &json!({"path":"indexed-replace-source.docx"}),
        &state,
        &request,
    )
    .expect("inspect indexed replacement paragraphs");
    let paragraphs = inspected
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .expect("top-level paragraphs");
    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[1].get("text").and_then(Value::as_str), Some(""));
    assert!(paragraphs.iter().all(|paragraph| {
        paragraph
            .get("eligible_for_index_replacement")
            .and_then(Value::as_bool)
            == Some(true)
    }));

    let replaced_empty = docx_edit::replace_docx_paragraph_at_index_with_content(
        &json!({
            "path":"indexed-replace-source.docx",
            "paragraph":2,
            "expected_text":"",
            "blocks":[
                {"type":"paragraph","text":"Replacement heading","style":"heading2"},
                {"type":"table","header_row":true,"rows":[["Name","Value"],["A","1"]]},
                {"type":"page_break"}
            ],
            "target_path":"replaced-indexed-empty.docx"
        }),
        &state,
        &request,
    )
    .expect("replace indexed empty paragraph");
    assert_eq!(
        replaced_empty.get("operation").and_then(Value::as_str),
        Some("replace_paragraph_at_index_with_content")
    );
    assert_eq!(
        replaced_empty.get("paragraph").and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        replaced_empty
            .get("expected_characters")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        replaced_empty
            .get("top_level_paragraphs_before")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        replaced_empty
            .get("top_level_paragraphs_after")
            .and_then(Value::as_u64),
        Some(4)
    );
    let mut empty_archive = ZipArchive::new(
        File::open(root.join("replaced-indexed-empty.docx")).expect("empty replacement output"),
    )
    .expect("empty replacement ZIP");
    let empty_output =
        read_zip_text(&mut empty_archive, "word/document.xml").expect("empty replacement XML");
    let first_index = empty_output
        .find(repeated_first)
        .expect("first repeated paragraph");
    let heading_index = empty_output
        .find("Replacement heading")
        .expect("replacement heading");
    let replacement_table_index = empty_output.rfind("<w:tbl>").expect("replacement table");
    let page_break_index = empty_output
        .find("<w:br w:type=\"page\"/>")
        .expect("replacement page break");
    let second_index = empty_output
        .find(repeated_second)
        .expect("second repeated paragraph");
    assert!(
        first_index < heading_index
            && heading_index < replacement_table_index
            && replacement_table_index < page_break_index
            && page_break_index < second_index
    );
    assert!(!empty_output.contains(empty));
    assert!(empty_output.contains("Nested table paragraph"));
    assert_eq!(
        read_zip_text(&mut empty_archive, "word/custom.xml").expect("preserved custom XML"),
        "<custom>indexed-replacement-preserved</custom>"
    );

    let replaced_repeated = docx_edit::replace_docx_paragraph_at_index_with_content(
        &json!({
            "path":"indexed-replace-source.docx",
            "paragraph":3,
            "expected_text":"Repeated",
            "blocks":[{"type":"page_break"}],
            "target_path":"replaced-indexed-repeated.docx"
        }),
        &state,
        &request,
    )
    .expect("replace specifically indexed repeated paragraph");
    assert_eq!(
        replaced_repeated
            .get("top_level_paragraphs_after")
            .and_then(Value::as_u64),
        Some(3)
    );
    let page_break = "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";
    let mut repeated_archive = ZipArchive::new(
        File::open(root.join("replaced-indexed-repeated.docx"))
            .expect("repeated replacement output"),
    )
    .expect("repeated replacement ZIP");
    let repeated_output = read_zip_text(&mut repeated_archive, "word/document.xml")
        .expect("repeated replacement XML");
    assert_eq!(
        repeated_output,
        document_xml.replacen(repeated_second, page_break, 1)
    );
    assert!(repeated_output.contains(repeated_first));
    assert_eq!(
        fs::read(root.join("indexed-replace-source.docx")).expect("source after replacements"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn indexed_docx_paragraph_replacement_rejects_mismatch_ranges_complex_noop_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: &str| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("indexed replacement rejection source");
    };
    write_source(
        "replace-index-valid.docx",
        "<w:p><w:r><w:t>First</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>Last</w:t></w:r></w:p>",
    );
    write_source(
        "replace-index-range.docx",
        "<w:bookmarkStart w:id=\"1\" w:name=\"range\"/><w:p><w:r><w:t>First</w:t></w:r></w:p><w:bookmarkEnd w:id=\"1\"/>",
    );
    write_source(
        "replace-index-hyperlink.docx",
        "<w:p><w:hyperlink w:anchor=\"target\"><w:r><w:t>Linked</w:t></w:r></w:hyperlink></w:p>",
    );
    write_source(
        "replace-index-section.docx",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Section</w:t></w:r></w:p>",
    );
    write_source(
        "replace-index-wrapper.docx",
        "<w:p><w:altChunk w:id=\"rId1\"/></w:p>",
    );
    write_source(
        "replace-index-malformed.docx",
        "<w:p><w:r><w:t>Broken</w:t></w:r>",
    );
    let identical_paragraph = r#"<w:p><w:pPr><w:pStyle w:val="Normal"/><w:jc w:val="left"/><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr><w:t xml:space="preserve">Anchor</w:t></w:r></w:p>"#;
    write_source("replace-index-noop.docx", identical_paragraph);
    let source_before = fs::read(root.join("replace-index-valid.docx")).expect("valid source");

    let range_inspection = inspect_docx(
        &json!({"path":"replace-index-range.docx"}),
        &state,
        &request,
    )
    .expect("inspect range-marked replacement document");
    assert!(range_inspection
        .get("top_level_paragraphs")
        .and_then(Value::as_array)
        .is_some_and(|paragraphs| paragraphs.iter().all(|paragraph| {
            [
                "eligible_for_index_insertion",
                "eligible_for_index_deletion",
                "eligible_for_index_replacement",
            ]
            .iter()
            .all(|field| paragraph.get(*field).and_then(Value::as_bool) == Some(false))
        })));

    let blocks = json!([{"type":"paragraph","text":"Replacement"}]);
    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"replace-index-valid.docx","paragraph":1,"expected_text":"Wrong","blocks":blocks,"target_path":"replace-index-mismatch.docx"}),
            "does not match expected_text",
        ),
        (
            1,
            json!({"path":"replace-index-valid.docx","paragraph":0,"expected_text":"First","blocks":blocks,"target_path":"replace-index-zero.docx"}),
            "paragraph must be an integer between 1 and 2000",
        ),
        (
            2,
            json!({"path":"replace-index-valid.docx","paragraph":4,"expected_text":"Last","blocks":blocks,"target_path":"replace-index-outside.docx"}),
            "paragraph index 4",
        ),
        (
            3,
            json!({"path":"replace-index-range.docx","paragraph":1,"expected_text":"First","blocks":blocks,"target_path":"replace-index-range-output.docx"}),
            "document range markup",
        ),
        (
            4,
            json!({"path":"replace-index-hyperlink.docx","paragraph":1,"expected_text":"Linked","blocks":blocks,"target_path":"replace-index-hyperlink-output.docx"}),
            "hyperlink",
        ),
        (
            5,
            json!({"path":"replace-index-section.docx","paragraph":1,"expected_text":"Section","blocks":blocks,"target_path":"replace-index-section-output.docx"}),
            "section properties",
        ),
        (
            6,
            json!({"path":"replace-index-wrapper.docx","paragraph":1,"expected_text":"","blocks":blocks,"target_path":"replace-index-wrapper-output.docx"}),
            "wrapper element",
        ),
        (
            7,
            json!({"path":"replace-index-malformed.docx","paragraph":1,"expected_text":"Broken","blocks":blocks,"target_path":"replace-index-malformed-output.docx"}),
            "mismatched element boundaries",
        ),
        (
            8,
            json!({"path":"replace-index-noop.docx","paragraph":1,"expected_text":"Anchor","blocks":[{"type":"paragraph","text":"Anchor"}],"target_path":"replace-index-noop-output.docx"}),
            "identical to the selected DOCX paragraph",
        ),
    ] {
        let error =
            docx_edit::replace_docx_paragraph_at_index_with_content(&arguments, &state, &request)
                .expect_err("unsafe indexed paragraph replacement must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: unexpected error: {error:#}"
        );
    }
    let control = docx_edit::replace_docx_paragraph_at_index_with_content(
        &json!({
            "path":"replace-index-valid.docx",
            "paragraph":1,
            "expected_text":"bad\u{0000}text",
            "blocks":[{"type":"paragraph","text":"Replacement"}],
            "target_path":"replace-index-control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("control expected text must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::replace_docx_paragraph_at_index_with_content(
        &json!({
            "path":"replace-index-valid.docx",
            "paragraph":1,
            "expected_text":"First",
            "blocks":[{"type":"paragraph","text":"Replacement"}],
            "target_path":"replace-index-valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place indexed paragraph replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(root.join("replace-index-valid.docx")).expect("valid source after rejections"),
        source_before
    );
    for target in [
        "replace-index-mismatch.docx",
        "replace-index-zero.docx",
        "replace-index-outside.docx",
        "replace-index-range-output.docx",
        "replace-index-hyperlink-output.docx",
        "replace-index-section-output.docx",
        "replace-index-wrapper-output.docx",
        "replace-index-malformed-output.docx",
        "replace-index-noop-output.docx",
        "replace-index-control.docx",
    ] {
        assert!(!root.join(target).exists(), "unexpected output {target}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaces_unique_adjacent_same_format_docx_text_across_runs_without_modifying_source() {
    let (root, state, request) = test_context();
    let run_properties = r#"<w:rPr><w:b/><w:color w:val="336699"/></w:rPr>"#;
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r>{run_properties}<w:t xml:space="preserve">Prefix Con</w:t></w:r><w:r>{run_properties}<w:t xml:space="preserve">tract &amp; </w:t></w:r><w:r>{run_properties}<w:t xml:space="preserve">Review suffix</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#
    );
    write_zip(
        root.join("split-format.docx").as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            (
                "_rels/.rels".to_string(),
                office_root_relationships("word/document.xml"),
            ),
            ("word/document.xml".to_string(), document_xml.clone()),
            (
                "word/_rels/document.xml.rels".to_string(),
                empty_relationships(),
            ),
            (
                "word/custom.xml".to_string(),
                "<custom>unchanged</custom>".to_string(),
            ),
        ],
        false,
    )
    .expect("split-format DOCX");
    let source = root.join("split-format.docx");
    let source_before = fs::read(source.as_path()).expect("source DOCX bytes");

    let replaced = docx_edit::replace_docx_text_across_runs(
        &json!({
            "path":"split-format.docx",
            "selection":"Contract & Review",
            "replacement":"合同 & 已审阅",
            "target_path":"replaced-cross-run.docx"
        }),
        &state,
        &request,
    )
    .expect("replace adjacent same-format runs");
    assert_eq!(
        replaced.get("operation").and_then(Value::as_str),
        Some("replace_text_across_runs")
    );
    assert_eq!(
        replaced.get("runs_touched").and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        replaced.get("emptied_runs").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        replaced.get("same_run_properties").and_then(Value::as_bool),
        Some(true)
    );

    let mut output_archive =
        ZipArchive::new(File::open(root.join("replaced-cross-run.docx")).expect("output DOCX"))
            .expect("output ZIP");
    let updated_xml =
        read_zip_text(&mut output_archive, "word/document.xml").expect("updated document XML");
    let custom = read_zip_text(&mut output_archive, "word/custom.xml").expect("custom XML");
    assert_eq!(custom, "<custom>unchanged</custom>");
    assert_eq!(updated_xml.matches(run_properties).count(), 3);
    assert!(updated_xml.contains(r#"<w:t xml:space="preserve">Prefix 合同 &amp; 已审阅</w:t>"#));
    assert!(updated_xml.contains(r#"<w:t xml:space="preserve"></w:t>"#));
    assert!(updated_xml.contains(r#"<w:t xml:space="preserve"> suffix</w:t>"#));
    let inspected = inspect_docx(&json!({"path":"replaced-cross-run.docx"}), &state, &request)
        .expect("inspect cross-run replacement");
    assert!(inspected
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Prefix 合同 & 已审阅") && text.contains("suffix")
        }));
    assert_eq!(
        fs::read(source.as_path()).expect("source after replacement"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn docx_cross_run_replacement_rejects_single_mixed_complex_duplicate_oversize_and_in_place() {
    let (root, state, request) = test_context();
    let write_source = |name: &str, body: String| {
        write_zip(
            root.join(name).as_path(),
            vec![
                ("[Content_Types].xml".to_string(), docx_content_types()),
                (
                    "_rels/.rels".to_string(),
                    office_root_relationships("word/document.xml"),
                ),
                (
                    "word/document.xml".to_string(),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
                    ),
                ),
                (
                    "word/_rels/document.xml.rels".to_string(),
                    empty_relationships(),
                ),
            ],
            false,
        )
        .expect("write cross-run source");
    };
    write_source(
        "single.docx",
        "<w:p><w:r><w:t>Hello</w:t></w:r></w:p>".to_string(),
    );
    write_source(
        "mixed.docx",
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hel</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>lo</w:t></w:r></w:p>".to_string(),
    );
    write_source(
        "complex.docx",
        "<w:p><w:r><w:t>Hel</w:t></w:r><w:bookmarkStart w:id=\"1\" w:name=\"mark\"/><w:r><w:t>lo</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p>".to_string(),
    );
    write_source(
        "duplicate.docx",
        "<w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p><w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p>".to_string(),
    );
    write_source(
        "valid.docx",
        "<w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo</w:t></w:r></w:p>".to_string(),
    );
    let too_many_runs = (0..17)
        .map(|_| "<w:r><w:t>A</w:t></w:r>")
        .collect::<String>();
    write_source("too-many.docx", format!("<w:p>{too_many_runs}</w:p>"));

    for (source, selection, expected) in [
        ("single.docx", "Hello", "use replace_docx_text instead"),
        ("mixed.docx", "Hello", "different run properties"),
        ("complex.docx", "Hello", "bookmark"),
        ("duplicate.docx", "Hello", "exactly once"),
        ("too-many.docx", "AAAAAAAAAAAAAAAAA", "16 run safety limit"),
    ] {
        let target = format!("output-{source}");
        let error = docx_edit::replace_docx_text_across_runs(
            &json!({
                "path":source,
                "selection":selection,
                "replacement":"World",
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsupported cross-run replacement must fail");
        assert!(
            error.to_string().contains(expected),
            "unexpected error for {source}: {error:#}"
        );
        assert!(!root.join(format!("output-{source}")).exists());
    }

    let noop = docx_edit::replace_docx_text_across_runs(
        &json!({
            "path":"mixed.docx",
            "selection":"Hello",
            "replacement":"Hello",
            "target_path":"noop.docx"
        }),
        &state,
        &request,
    )
    .expect_err("cross-run no-op must fail");
    assert!(noop.to_string().contains("must change"));
    let control = docx_edit::replace_docx_text_across_runs(
        &json!({
            "path":"mixed.docx",
            "selection":"Hello",
            "replacement":"bad\u{0000}text",
            "target_path":"control.docx"
        }),
        &state,
        &request,
    )
    .expect_err("XML control must fail");
    assert!(control.to_string().contains("XML-incompatible control"));
    let in_place = docx_edit::replace_docx_text_across_runs(
        &json!({
            "path":"valid.docx",
            "selection":"Hello",
            "replacement":"World",
            "target_path":"valid.docx",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place cross-run replacement must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert!(!root.join("noop.docx").exists());
    assert!(!root.join("control.docx").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn artifact_template_verifies_and_instantiates_local_source() {
    let (root, state, request) = test_context();
    create_csv(
        &json!({"target_path":"artifacts/source.csv","rows":[["a","b"],[1,2]]}),
        &state,
        &request,
    )
    .expect("csv");
    create_artifact_template(
        &json!({
            "source_path":"artifacts/source.csv",
            "target_directory":"templates/demo",
            "template_name":"Demo"
        }),
        &state,
        &request,
    )
    .expect("template");
    let inspected = inspect_artifact_template(
        &json!({"template_directory":"templates/demo"}),
        &state,
        &request,
    )
    .expect("inspect template");
    assert_eq!(
        inspected.get("hash_valid").and_then(Value::as_bool),
        Some(true)
    );
    instantiate_artifact_template(
        &json!({"template_directory":"templates/demo","target_path":"artifacts/copy.csv"}),
        &state,
        &request,
    )
    .expect("instantiate");
    assert!(root.join("artifacts/copy.csv").is_file());
    let preview_error = render_artifact_template_preview(
        &json!({"template_directory":"templates/demo"}),
        &state,
        &request,
        None,
    )
    .expect_err("CSV template preview must fail closed");
    assert!(preview_error
        .to_string()
        .contains("template_render/artifact_unsupported"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_templates_instantiate_docx_pptx_and_xlsx_placeholders() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({
            "target_path":"artifacts/source.docx",
            "title":"Report for {{CLIENT}}",
            "paragraphs":["Owner: {{OWNER}}"]
        }),
        &state,
        &request,
    )
    .expect("DOCX source");
    presentation::create_pptx(
        &json!({
            "target_path":"artifacts/source.pptx",
            "slides":[{"title":"Welcome {{CLIENT}}","body":"Owner: {{OWNER}}"}]
        }),
        &state,
        &request,
    )
    .expect("PPTX source");
    spreadsheet::create_xlsx(
        &json!({
            "target_path":"artifacts/source.xlsx",
            "rows":[["Client","{{CLIENT}}"],["Owner","{{OWNER}}"]]
        }),
        &state,
        &request,
    )
    .expect("XLSX source");

    for extension in ["docx", "pptx", "xlsx"] {
        let source = format!("artifacts/source.{extension}");
        let template = format!("templates/{extension}");
        let output = format!("artifacts/output.{extension}");
        let source_before = fs::read(root.join(source.as_str())).expect("source bytes");
        create_artifact_template(
            &json!({
                "source_path":source,
                "target_directory":template,
                "template_name":format!("{extension} template"),
                "placeholders":[
                    {"name":"CLIENT","description":"Client name","max_length":100},
                    {"name":"OWNER","required":false,"default":"Unassigned","max_length":100}
                ]
            }),
            &state,
            &request,
        )
        .expect("semantic template");
        let inspected = inspect_artifact_template(
            &json!({"template_directory":format!("templates/{extension}")}),
            &state,
            &request,
        )
        .expect("inspect semantic template");
        assert_eq!(
            inspected.get("placeholder_valid").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            inspected.get("placeholder_count").and_then(Value::as_u64),
            Some(2)
        );
        let instantiated = instantiate_artifact_template(
            &json!({
                "template_directory":format!("templates/{extension}"),
                "target_path":output,
                "values":{"CLIENT":"Acme & Co"}
            }),
            &state,
            &request,
        )
        .expect("instantiate semantic template");
        assert_eq!(
            instantiated.get("replacements").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            fs::read(root.join(format!("artifacts/source.{extension}"))).expect("source after"),
            source_before
        );
    }

    let docx = inspect_docx(&json!({"path":"artifacts/output.docx"}), &state, &request)
        .expect("inspect templated DOCX");
    assert!(docx
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Acme & Co") && value.contains("Unassigned")));
    let pptx =
        presentation::inspect_pptx(&json!({"path":"artifacts/output.pptx"}), &state, &request)
            .expect("inspect templated PPTX");
    assert!(pptx
        .pointer("/slide_metadata/0/text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Acme & Co") && value.contains("Unassigned")));
    let mut xlsx =
        ZipArchive::new(File::open(root.join("artifacts/output.xlsx")).expect("XLSX output"))
            .expect("XLSX ZIP");
    let sheet = read_zip_text(&mut xlsx, "xl/worksheets/sheet1.xml").expect("XLSX sheet");
    assert!(sheet.contains("Acme &amp; Co"));
    assert!(sheet.contains("Unassigned"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_templates_reject_missing_split_and_unknown_placeholders() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({
            "target_path":"source.docx",
            "title":"Hello {{NAME}}",
            "paragraphs":[]
        }),
        &state,
        &request,
    )
    .expect("DOCX source");
    create_artifact_template(
        &json!({
            "source_path":"source.docx",
            "target_directory":"template",
            "template_name":"Required value",
            "placeholders":[{"name":"NAME"}]
        }),
        &state,
        &request,
    )
    .expect("template");
    let missing = instantiate_artifact_template(
        &json!({"template_directory":"template","target_path":"missing.docx"}),
        &state,
        &request,
    )
    .expect_err("missing required value must fail");
    assert!(missing
        .to_string()
        .contains("required template placeholder"));
    let unknown = instantiate_artifact_template(
        &json!({
            "template_directory":"template",
            "target_path":"unknown.docx",
            "values":{"NAME":"Ada","EXTRA":"rejected"}
        }),
        &state,
        &request,
    )
    .expect_err("unknown value must fail");
    assert!(unknown.to_string().contains("unknown placeholder"));

    let split_source = root.join("split.docx");
    write_zip(
        split_source.as_path(),
        vec![
            ("[Content_Types].xml".to_string(), docx_content_types()),
            ("_rels/.rels".to_string(), office_root_relationships("word/document.xml")),
            (
                "word/document.xml".to_string(),
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{{NA</w:t></w:r><w:r><w:t>ME}}</w:t></w:r></w:p></w:body></w:document>"#.to_string(),
            ),
        ],
        false,
    )
    .expect("split DOCX");
    let split = create_artifact_template(
        &json!({
            "source_path":"split.docx",
            "target_directory":"split-template",
            "template_name":"Split",
            "placeholders":[{"name":"NAME"}]
        }),
        &state,
        &request,
    )
    .expect_err("split token must fail");
    assert!(split.to_string().contains("not found inside a single"));
    assert!(!root.join("missing.docx").exists());
    assert!(!root.join("unknown.docx").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creates_paginated_ascii_text_pdf_with_metadata_and_page_numbers() {
    let (root, state, request) = test_context();
    let paragraphs = (1..=120)
        .map(|index| {
            format!(
                "Paragraph {index}: This locally generated PDF line is intentionally long enough to exercise bounded Helvetica wrapping and pagination."
            )
        })
        .collect::<Vec<_>>();
    let created = pdf_edit::create_text_pdf(
        &json!({
            "target_path":"artifacts/generated.pdf",
            "title":"Local PDF Generation",
            "paragraphs":paragraphs,
            "page_size":"letter",
            "font_size":11,
            "title_font_size":22,
            "line_spacing":1.2,
            "margin_points":48,
            "page_numbers":true,
            "author":"ChatOS Test",
            "subject":"Bounded native PDF generation"
        }),
        &state,
        &request,
    )
    .expect("create text PDF");
    let page_count = created
        .get("pages")
        .and_then(Value::as_u64)
        .expect("generated pages");
    assert!(page_count >= 3);
    assert_eq!(
        created.get("page_size").and_then(Value::as_str),
        Some("letter")
    );
    assert_eq!(
        created.get("text_encoding").and_then(Value::as_str),
        Some("printable_ascii")
    );

    let generated_path = root.join("artifacts/generated.pdf");
    let document = Document::load(generated_path.as_path()).expect("generated PDF");
    assert_eq!(document.get_pages().len(), page_count as usize);
    let first_page_id = document
        .get_pages()
        .get(&1)
        .copied()
        .expect("generated first page");
    let pages_root_id = document
        .get_object(first_page_id)
        .and_then(Object::as_dict)
        .and_then(|page| page.get(b"Parent"))
        .and_then(Object::as_reference)
        .expect("generated pages root");
    let media_box = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .and_then(|pages| pages.get(b"MediaBox"))
        .and_then(Object::as_array)
        .expect("generated MediaBox");
    assert_eq!(media_box[2].as_float().expect("MediaBox width"), 612.0);
    assert_eq!(media_box[3].as_float().expect("MediaBox height"), 792.0);
    let info_id = document
        .trailer
        .get(b"Info")
        .and_then(Object::as_reference)
        .expect("PDF info dictionary");
    let info = document
        .get_object(info_id)
        .and_then(Object::as_dict)
        .expect("PDF metadata");
    let author = info
        .get(b"Author")
        .and_then(Object::as_str)
        .expect("PDF author metadata");
    assert_eq!(String::from_utf8_lossy(author), "ChatOS Test");
    let inspected = inspect_pdf(&json!({"path":"artifacts/generated.pdf"}), &state, &request)
        .expect("inspect generated PDF");
    assert_eq!(
        inspected.get("pages").and_then(Value::as_u64),
        Some(page_count)
    );
    assert_eq!(
        inspected.pointer("/metadata/title").and_then(Value::as_str),
        Some("Local PDF Generation")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/author")
            .and_then(Value::as_str),
        Some("ChatOS Test")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/subject")
            .and_then(Value::as_str),
        Some("Bounded native PDF generation")
    );
    let extracted = extract_pdf_text(
        &json!({"path":"artifacts/generated.pdf","max_chars":500000}),
        &state,
        &request,
    )
    .expect("extract generated PDF text");
    assert!(extracted
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|text| {
            text.contains("Local PDF Generation")
                && text.contains("Paragraph 120")
                && text.contains("Page 1 of")
        }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn text_pdf_generation_rejects_unicode_invalid_layout_and_unapproved_overwrite() {
    let (root, state, request) = test_context();
    let unicode = pdf_edit::create_text_pdf(
        &json!({
            "target_path":"unicode.pdf",
            "paragraphs":["中文内容"]
        }),
        &state,
        &request,
    )
    .expect_err("Unicode without embedded font must fail");
    assert!(unicode.to_string().contains("printable ASCII"));
    assert!(!root.join("unicode.pdf").exists());

    let invalid_layout = pdf_edit::create_text_pdf(
        &json!({
            "target_path":"invalid-layout.pdf",
            "paragraphs":["Text"],
            "font_size":30
        }),
        &state,
        &request,
    )
    .expect_err("out-of-range font size must fail");
    assert!(invalid_layout
        .to_string()
        .contains("font_size must be between"));
    assert!(!root.join("invalid-layout.pdf").exists());

    pdf_edit::create_text_pdf(
        &json!({
            "target_path":"existing.pdf",
            "paragraphs":["First version"]
        }),
        &state,
        &request,
    )
    .expect("initial PDF");
    let existing_before = fs::read(root.join("existing.pdf")).expect("existing PDF bytes");
    let overwrite = pdf_edit::create_text_pdf(
        &json!({
            "target_path":"existing.pdf",
            "paragraphs":["Second version"]
        }),
        &state,
        &request,
    )
    .expect_err("overwrite must require approval");
    assert!(overwrite.to_string().contains("overwrite=true"));
    assert_eq!(
        fs::read(root.join("existing.pdf")).expect("existing PDF after rejection"),
        existing_before
    );
    let empty = pdf_edit::create_text_pdf(
        &json!({
            "target_path":"empty.pdf",
            "paragraphs":[]
        }),
        &state,
        &request,
    )
    .expect_err("empty PDF input must fail");
    assert!(empty.to_string().contains("between 1 and"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn updates_and_inspects_unicode_pdf_metadata_while_preserving_unrelated_info_fields() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 2);
    let mut prepared = Document::load(source.as_path()).expect("source PDF");
    let info_id = prepared.add_object(dictionary! {
        "Title" => lopdf::text_string("Old title"),
        "Subject" => lopdf::text_string("Remove this subject"),
        "Creator" => lopdf::text_string("Existing Creator"),
        "Producer" => lopdf::text_string("Existing Producer"),
        "CustomField" => lopdf::text_string("Preserve custom value"),
    });
    prepared.trailer.set("Info", info_id);
    prepared.save(source.as_path()).expect("save source PDF");
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    let updated = pdf_edit::update_pdf_metadata(
        &json!({
            "path":"artifacts/source.pdf",
            "title":"合同审阅版本",
            "author":"李雷",
            "keywords":"合同, 审阅, 2026",
            "remove_fields":["subject"],
            "target_path":"artifacts/metadata.pdf"
        }),
        &state,
        &request,
    )
    .expect("update PDF metadata");
    assert_eq!(
        updated.get("operation").and_then(Value::as_str),
        Some("update_metadata")
    );
    assert_eq!(
        updated.get("updated_fields"),
        Some(&json!(["title", "author", "keywords"]))
    );
    assert_eq!(updated.get("removed_fields"), Some(&json!(["subject"])));

    let inspected = inspect_pdf(&json!({"path":"artifacts/metadata.pdf"}), &state, &request)
        .expect("inspect updated PDF metadata");
    assert_eq!(
        inspected.pointer("/metadata/title").and_then(Value::as_str),
        Some("合同审阅版本")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/author")
            .and_then(Value::as_str),
        Some("李雷")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/keywords")
            .and_then(Value::as_str),
        Some("合同, 审阅, 2026")
    );
    assert!(inspected
        .pointer("/metadata/subject")
        .is_some_and(Value::is_null));
    assert_eq!(
        inspected
            .pointer("/metadata/creator")
            .and_then(Value::as_str),
        Some("Existing Creator")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/producer")
            .and_then(Value::as_str),
        Some("Existing Producer")
    );
    assert_eq!(
        inspected
            .pointer("/metadata/other_field_count")
            .and_then(Value::as_u64),
        Some(1)
    );

    let output = Document::load(root.join("artifacts/metadata.pdf")).expect("metadata PDF");
    let info_id = output
        .trailer
        .get(b"Info")
        .and_then(Object::as_reference)
        .expect("output Info reference");
    let info = output
        .get_object(info_id)
        .and_then(Object::as_dict)
        .expect("output Info dictionary");
    assert!(!info.has(b"Subject"));
    assert_eq!(
        lopdf::decode_text_string(info.get(b"CustomField").expect("custom field"))
            .expect("decode custom field"),
        "Preserve custom value"
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after metadata update"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_metadata_update_rejects_missing_overlap_noop_controls_malformed_info_and_in_place() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_blank_pdf(source.as_path(), 1);
    let mut prepared = Document::load(source.as_path()).expect("source PDF");
    let info_id = prepared.add_object(dictionary! {
        "Title" => lopdf::text_string("Same title"),
        "Producer" => lopdf::text_string("Keep producer"),
    });
    prepared.trailer.set("Info", info_id);
    prepared.save(source.as_path()).expect("save source PDF");
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    for (target, values, expected) in [
        (
            "missing.pdf",
            json!({}),
            "requires at least one field value",
        ),
        (
            "overlap.pdf",
            json!({"title":"New title","remove_fields":["title"]}),
            "cannot be both updated and removed",
        ),
        (
            "noop.pdf",
            json!({"title":"Same title"}),
            "would not change",
        ),
        (
            "control.pdf",
            json!({"title":"bad\u{0000}title"}),
            "unsupported control character",
        ),
        ("type.pdf", json!({"author":7}), "author must be a string"),
        (
            "empty-remove.pdf",
            json!({"remove_fields":[]}),
            "between 1 and 4",
        ),
        (
            "duplicate-remove.pdf",
            json!({"remove_fields":["title","title"]}),
            "must be unique",
        ),
        (
            "unknown-remove.pdf",
            json!({"remove_fields":["producer"]}),
            "title, author, subject, or keywords",
        ),
    ] {
        let mut arguments = values;
        arguments["path"] = json!("source.pdf");
        arguments["target_path"] = json!(target);
        let error = pdf_edit::update_pdf_metadata(&arguments, &state, &request)
            .expect_err("invalid metadata update must fail");
        assert!(
            error.to_string().contains(expected),
            "unexpected error for {target}: {error}"
        );
        assert!(!root.join(target).exists());
    }

    let in_place = pdf_edit::update_pdf_metadata(
        &json!({
            "path":"source.pdf",
            "title":"Changed title",
            "target_path":"source.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("in-place metadata update must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after rejected updates"),
        source_before
    );

    let malformed = root.join("malformed.pdf");
    write_blank_pdf(malformed.as_path(), 1);
    let mut document = Document::load(malformed.as_path()).expect("malformed PDF");
    document.trailer.set("Info", 9);
    document
        .save(malformed.as_path())
        .expect("save malformed PDF");
    let malformed_error = pdf_edit::update_pdf_metadata(
        &json!({
            "path":"malformed.pdf",
            "title":"Changed title",
            "target_path":"malformed-output.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("malformed Info must fail");
    assert!(malformed_error
        .to_string()
        .contains("Info must be a dictionary"));
    assert!(!root.join("malformed-output.pdf").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspects_and_fills_all_supported_exact_acroform_values() {
    let (root, state, request) = test_context();
    let source = root.join("forms/source.pdf");
    write_acroform_pdf(source.as_path());
    let source_before = fs::read(source.as_path()).expect("source AcroForm PDF bytes");

    let inspected = inspect_pdf(&json!({"path":"forms/source.pdf"}), &state, &request)
        .expect("inspect AcroForm PDF");
    assert_eq!(
        inspected.pointer("/form/present").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        inspected
            .pointer("/form/field_count")
            .and_then(Value::as_u64),
        Some(6)
    );
    assert_eq!(
        inspected
            .pointer("/form/fillable_field_count")
            .and_then(Value::as_u64),
        Some(6)
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/0/current_value")
            .and_then(Value::as_str),
        Some("Alice")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/1/current_value")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/2/current_value")
            .and_then(Value::as_str),
        Some("Basic")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/2/options/1/value")
            .and_then(Value::as_str),
        Some("Premium")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/3/current_value")
            .and_then(Value::as_str),
        Some("cn")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/3/options/0/label")
            .and_then(Value::as_str),
        Some("中国")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/3/choice_style")
            .and_then(Value::as_str),
        Some("combo")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/4/current_value")
            .and_then(Value::as_str),
        Some("上海")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/4/choice_style")
            .and_then(Value::as_str),
        Some("editable_combo")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/5/current_value/0")
            .and_then(Value::as_str),
        Some("red")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/5/current_value/1")
            .and_then(Value::as_str),
        Some("blue")
    );
    assert_eq!(
        inspected
            .pointer("/form/preview/5/choice_style")
            .and_then(Value::as_str),
        Some("multi_select_list")
    );

    let filled = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"forms/source.pdf",
            "fields":[
                {"name":"profile.name","expected_value":"Alice","value":"李雷"},
                {"name":"terms.accepted","expected_value":false,"value":true},
                {"name":"subscription.plan","expected_value":"Basic","value":"Premium"},
                {"name":"profile.region","expected_value":"cn","value":"us"},
                {"name":"profile.city","expected_value":"上海","value":"深圳"},
                {"name":"preferences.colors","expected_value":["red","blue"],"value":["green","blue"]}
            ],
            "target_path":"forms/filled.pdf"
        }),
        &state,
        &request,
    )
    .expect("fill AcroForm PDF");
    assert_eq!(
        filled.get("operation").and_then(Value::as_str),
        Some("fill_form_fields")
    );
    assert_eq!(
        filled.get("updated_field_count").and_then(Value::as_u64),
        Some(6)
    );
    assert_eq!(
        filled.get("appearance_mode").and_then(Value::as_str),
        Some("viewer_regeneration_requested")
    );

    let output = Document::load(root.join("forms/filled.pdf")).expect("filled AcroForm PDF");
    let form = pdf_edit::inspect_pdf_form(&output).expect("inspect filled AcroForm values");
    assert_eq!(
        form.pointer("/preview/0/current_value")
            .and_then(Value::as_str),
        Some("李雷")
    );
    assert_eq!(
        form.pointer("/preview/1/current_value")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        form.pointer("/preview/2/current_value")
            .and_then(Value::as_str),
        Some("Premium")
    );
    assert_eq!(
        form.pointer("/preview/3/current_value")
            .and_then(Value::as_str),
        Some("us")
    );
    assert_eq!(
        form.pointer("/preview/4/current_value")
            .and_then(Value::as_str),
        Some("深圳")
    );
    assert_eq!(
        form.pointer("/preview/5/current_value/0")
            .and_then(Value::as_str),
        Some("green")
    );
    assert_eq!(
        form.pointer("/preview/5/current_value/1")
            .and_then(Value::as_str),
        Some("blue")
    );
    assert_eq!(
        form.get("need_appearances").and_then(Value::as_bool),
        Some(true)
    );
    let catalog = output.catalog().expect("filled PDF catalog");
    let acroform_id = catalog
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("AcroForm reference");
    let acroform = output
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("AcroForm dictionary");
    let fields = acroform
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("AcroForm fields");
    let text_field_id = fields[0].as_reference().expect("text field reference");
    let text_field = output
        .get_object(text_field_id)
        .and_then(Object::as_dict)
        .expect("text field");
    assert_eq!(
        lopdf::decode_text_string(text_field.get(b"V").expect("text field value"))
            .expect("decode text field value"),
        "李雷"
    );
    let text_widget_id = text_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("text field widgets")[0]
        .as_reference()
        .expect("text widget reference");
    assert!(!output
        .get_object(text_widget_id)
        .and_then(Object::as_dict)
        .expect("text widget")
        .has(b"AP"));
    let checkbox_field_id = fields[1].as_reference().expect("checkbox field reference");
    let checkbox_field = output
        .get_object(checkbox_field_id)
        .and_then(Object::as_dict)
        .expect("checkbox field");
    assert_eq!(
        checkbox_field
            .get(b"V")
            .and_then(Object::as_name)
            .expect("checkbox value"),
        b"Yes"
    );
    let checkbox_widget_id = checkbox_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("checkbox widgets")[0]
        .as_reference()
        .expect("checkbox widget reference");
    let checkbox_widget = output
        .get_object(checkbox_widget_id)
        .and_then(Object::as_dict)
        .expect("checkbox widget");
    assert_eq!(
        checkbox_widget
            .get(b"AS")
            .and_then(Object::as_name)
            .expect("checkbox appearance state"),
        b"Yes"
    );
    assert!(checkbox_widget.has(b"AP"));
    let radio_field_id = fields[2].as_reference().expect("radio field reference");
    let radio_field = output
        .get_object(radio_field_id)
        .and_then(Object::as_dict)
        .expect("radio field");
    assert_eq!(
        radio_field
            .get(b"V")
            .and_then(Object::as_name)
            .expect("radio value"),
        b"Premium"
    );
    let radio_widgets = radio_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("radio widgets");
    for (index, expected) in [b"Off".as_slice(), b"Premium".as_slice()]
        .into_iter()
        .enumerate()
    {
        let widget_id = radio_widgets[index]
            .as_reference()
            .expect("radio widget reference");
        let widget = output
            .get_object(widget_id)
            .and_then(Object::as_dict)
            .expect("radio widget");
        assert_eq!(
            widget
                .get(b"AS")
                .and_then(Object::as_name)
                .expect("radio appearance state"),
            expected
        );
        assert!(widget.has(b"AP"));
    }
    let choice_field_id = fields[3].as_reference().expect("choice field reference");
    let choice_field = output
        .get_object(choice_field_id)
        .and_then(Object::as_dict)
        .expect("choice field");
    assert_eq!(
        lopdf::decode_text_string(choice_field.get(b"V").expect("choice value"))
            .expect("decode choice value"),
        "us"
    );
    assert_eq!(
        choice_field
            .get(b"I")
            .and_then(Object::as_array)
            .expect("choice selected index")[0]
            .as_i64()
            .expect("choice selected index value"),
        1
    );
    let choice_widget_id = choice_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("choice widgets")[0]
        .as_reference()
        .expect("choice widget reference");
    assert!(!output
        .get_object(choice_widget_id)
        .and_then(Object::as_dict)
        .expect("choice widget")
        .has(b"AP"));
    let editable_choice_field_id = fields[4]
        .as_reference()
        .expect("editable choice field reference");
    let editable_choice_field = output
        .get_object(editable_choice_field_id)
        .and_then(Object::as_dict)
        .expect("editable choice field");
    assert_eq!(
        lopdf::decode_text_string(
            editable_choice_field
                .get(b"V")
                .expect("editable choice value")
        )
        .expect("decode editable choice value"),
        "深圳"
    );
    assert!(!editable_choice_field.has(b"I"));
    let editable_choice_widget_id = editable_choice_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("editable choice widgets")[0]
        .as_reference()
        .expect("editable choice widget reference");
    assert!(!output
        .get_object(editable_choice_widget_id)
        .and_then(Object::as_dict)
        .expect("editable choice widget")
        .has(b"AP"));
    let multi_choice_field_id = fields[5]
        .as_reference()
        .expect("multi-select choice field reference");
    let multi_choice_field = output
        .get_object(multi_choice_field_id)
        .and_then(Object::as_dict)
        .expect("multi-select choice field");
    let multi_choice_values = multi_choice_field
        .get(b"V")
        .and_then(Object::as_array)
        .expect("multi-select choice values")
        .iter()
        .map(|value| lopdf::decode_text_string(value).expect("decode multi-select choice value"))
        .collect::<Vec<_>>();
    assert_eq!(multi_choice_values, vec!["green", "blue"]);
    let multi_choice_indices = multi_choice_field
        .get(b"I")
        .and_then(Object::as_array)
        .expect("multi-select choice indices")
        .iter()
        .map(|value| value.as_i64().expect("multi-select choice index"))
        .collect::<Vec<_>>();
    assert_eq!(multi_choice_indices, vec![1, 2]);
    let multi_choice_widget_id = multi_choice_field
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("multi-select choice widgets")[0]
        .as_reference()
        .expect("multi-select choice widget reference");
    assert!(!output
        .get_object(multi_choice_widget_id)
        .and_then(Object::as_dict)
        .expect("multi-select choice widget")
        .has(b"AP"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after form fill"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clears_nullable_acroform_radio_and_choice_values() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_acroform_pdf(source.as_path());
    let source_before = fs::read(source.as_path()).expect("source AcroForm PDF bytes");

    pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"source.pdf",
            "fields":[
                {"name":"subscription.plan","expected_value":"Basic","value":null},
                {"name":"profile.region","expected_value":"cn","value":null},
                {"name":"profile.city","expected_value":"上海","value":null},
                {"name":"preferences.colors","expected_value":["red","blue"],"value":[]}
            ],
            "target_path":"cleared.pdf"
        }),
        &state,
        &request,
    )
    .expect("clear nullable AcroForm selections");

    let output = Document::load(root.join("cleared.pdf")).expect("cleared AcroForm PDF");
    let form = pdf_edit::inspect_pdf_form(&output).expect("inspect cleared AcroForm values");
    assert!(form
        .pointer("/preview/2/current_value")
        .is_some_and(Value::is_null));
    assert!(form
        .pointer("/preview/3/current_value")
        .is_some_and(Value::is_null));
    assert!(form
        .pointer("/preview/4/current_value")
        .is_some_and(Value::is_null));
    assert_eq!(
        form.pointer("/preview/5/current_value")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
    let catalog = output.catalog().expect("cleared PDF catalog");
    let acroform_id = catalog
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("AcroForm reference");
    let fields = output
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("AcroForm dictionary")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("AcroForm fields");
    let radio = output
        .get_object(fields[2].as_reference().expect("radio field reference"))
        .and_then(Object::as_dict)
        .expect("radio field");
    assert_eq!(
        radio
            .get(b"V")
            .and_then(Object::as_name)
            .expect("cleared radio value"),
        b"Off"
    );
    for widget in radio
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("radio widgets")
    {
        let widget = output
            .get_object(widget.as_reference().expect("radio widget reference"))
            .and_then(Object::as_dict)
            .expect("radio widget");
        assert_eq!(
            widget
                .get(b"AS")
                .and_then(Object::as_name)
                .expect("radio widget appearance state"),
            b"Off"
        );
    }
    let choice = output
        .get_object(fields[3].as_reference().expect("choice field reference"))
        .and_then(Object::as_dict)
        .expect("choice field");
    assert!(!choice.has(b"V"));
    assert!(!choice.has(b"I"));
    for index in [4_usize, 5_usize] {
        let choice = output
            .get_object(
                fields[index]
                    .as_reference()
                    .expect("choice field reference"),
            )
            .and_then(Object::as_dict)
            .expect("choice field");
        assert!(!choice.has(b"V"));
        assert!(!choice.has(b"I"));
    }
    assert_eq!(
        fs::read(source.as_path()).expect("source after cleared form output"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_form_fill_rejects_stale_unsafe_noop_and_xfa_requests() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_acroform_pdf(source.as_path());
    let source_before = fs::read(source.as_path()).expect("source AcroForm PDF bytes");

    for (target, field, expected) in [
        (
            "stale.pdf",
            json!({"name":"profile.name","expected_value":"Bob","value":"李雷"}),
            "expected_value does not match",
        ),
        (
            "wrong-type.pdf",
            json!({"name":"profile.name","expected_value":"Alice","value":true}),
            "text form field value must be a string",
        ),
        (
            "noop.pdf",
            json!({"name":"terms.accepted","expected_value":false,"value":false}),
            "would not change",
        ),
        (
            "unknown-radio.pdf",
            json!({"name":"subscription.plan","expected_value":"Basic","value":"Enterprise"}),
            "is not one of its verified options",
        ),
        (
            "wrong-choice-type.pdf",
            json!({"name":"profile.region","expected_value":"cn","value":true}),
            "choice form field value must be a string or null",
        ),
        (
            "editable-choice-control.pdf",
            json!({"name":"profile.city","expected_value":"上海","value":"深圳\n"}),
            "contains a control character",
        ),
        (
            "wrong-multi-choice-type.pdf",
            json!({"name":"preferences.colors","expected_value":["red","blue"],"value":"green"}),
            "multi-select choice form field value must be an array",
        ),
        (
            "unknown-multi-choice.pdf",
            json!({"name":"preferences.colors","expected_value":["red","blue"],"value":["red","purple"]}),
            "is not one of its exact options",
        ),
        (
            "unordered-multi-choice.pdf",
            json!({"name":"preferences.colors","expected_value":["red","blue"],"value":["blue","green"]}),
            "must follow exact option order",
        ),
    ] {
        let error = pdf_edit::fill_pdf_form_fields(
            &json!({
                "path":"source.pdf",
                "fields":[field],
                "target_path":target
            }),
            &state,
            &request,
        )
        .expect_err("unsafe form update must fail");
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!root.join(target).exists());
    }

    let in_place = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"source.pdf",
            "fields":[{"name":"profile.name","expected_value":"Alice","value":"Li Lei"}],
            "target_path":"source.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("in-place form update must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let no_toggle = root.join("no-toggle.pdf");
    fs::copy(source.as_path(), no_toggle.as_path()).expect("copy no-toggle fixture");
    let mut no_toggle_document = Document::load(no_toggle.as_path()).expect("load no-toggle PDF");
    let no_toggle_acroform_id = no_toggle_document
        .catalog()
        .expect("no-toggle catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("no-toggle AcroForm reference");
    let no_toggle_radio_id = no_toggle_document
        .get_object(no_toggle_acroform_id)
        .and_then(Object::as_dict)
        .expect("no-toggle AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("no-toggle fields")[2]
        .as_reference()
        .expect("no-toggle radio reference");
    no_toggle_document
        .get_object_mut(no_toggle_radio_id)
        .and_then(Object::as_dict_mut)
        .expect("no-toggle radio")
        .set("Ff", Object::Integer((1_i64 << 15) | (1_i64 << 14)));
    no_toggle_document
        .save(no_toggle.as_path())
        .expect("save no-toggle fixture");
    let no_toggle_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"no-toggle.pdf",
            "fields":[{"name":"subscription.plan","expected_value":"Basic","value":null}],
            "target_path":"no-toggle-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("NoToggleToOff radio clear must fail");
    assert!(no_toggle_error.to_string().contains("NoToggleToOff"));

    let multi_choice = root.join("multi-choice.pdf");
    fs::copy(source.as_path(), multi_choice.as_path()).expect("copy multi-choice fixture");
    let mut multi_choice_document =
        Document::load(multi_choice.as_path()).expect("load multi-choice PDF");
    let multi_choice_acroform_id = multi_choice_document
        .catalog()
        .expect("multi-choice catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("multi-choice AcroForm reference");
    let multi_choice_field_id = multi_choice_document
        .get_object(multi_choice_acroform_id)
        .and_then(Object::as_dict)
        .expect("multi-choice AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("multi-choice fields")[3]
        .as_reference()
        .expect("multi-choice field reference");
    let multi_choice_field = multi_choice_document
        .get_object_mut(multi_choice_field_id)
        .and_then(Object::as_dict_mut)
        .expect("multi-choice field");
    multi_choice_field.set("Ff", Object::Integer((1_i64 << 17) | (1_i64 << 21)));
    multi_choice_field.set(
        "V",
        Object::Array(vec![lopdf::text_string("cn"), lopdf::text_string("us")]),
    );
    multi_choice_field.set("I", vec![Object::Integer(0), Object::Integer(1)]);
    multi_choice_document
        .save(multi_choice.as_path())
        .expect("save multi-choice fixture");
    let inspected_multi_choice = inspect_pdf(&json!({"path":"multi-choice.pdf"}), &state, &request)
        .expect("inspect unsupported multi-select choice");
    assert_eq!(
        inspected_multi_choice
            .pointer("/form/preview/3/fillable")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        inspected_multi_choice
            .pointer("/form/preview/3/unsupported_reason")
            .and_then(Value::as_str),
        Some("multi-select choice field must be a list box")
    );
    let multi_choice_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"multi-choice.pdf",
            "fields":[{"name":"profile.region","expected_value":"cn","value":"us"}],
            "target_path":"multi-choice-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("multi-select choice fill must fail");
    assert!(multi_choice_error
        .to_string()
        .contains("multi-select choice field must be a list box"));

    let xfa = root.join("xfa.pdf");
    fs::copy(source.as_path(), xfa.as_path()).expect("copy XFA fixture");
    let mut xfa_document = Document::load(xfa.as_path()).expect("load XFA fixture");
    let acroform_id = xfa_document
        .catalog()
        .expect("XFA catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("XFA AcroForm reference");
    xfa_document
        .get_object_mut(acroform_id)
        .and_then(Object::as_dict_mut)
        .expect("XFA AcroForm")
        .set("XFA", lopdf::text_string("unsupported"));
    xfa_document.save(xfa.as_path()).expect("save XFA fixture");
    let xfa_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"xfa.pdf",
            "fields":[{"name":"profile.name","expected_value":"Alice","value":"Li Lei"}],
            "target_path":"xfa-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("XFA form fill must fail");
    assert!(xfa_error
        .to_string()
        .contains("XFA forms are not supported"));

    let signed = root.join("signed.pdf");
    fs::copy(source.as_path(), signed.as_path()).expect("copy signed-form fixture");
    let mut signed_document = Document::load(signed.as_path()).expect("load signed-form fixture");
    let acroform_id = signed_document
        .catalog()
        .expect("signed-form catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("signed-form AcroForm reference");
    let signature_id = signed_document.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
    });
    let signature_field_id = signed_document.add_object(dictionary! {
        "FT" => "Sig",
        "T" => lopdf::text_string("approval.signature"),
        "V" => signature_id,
    });
    let mut signed_fields = signed_document
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("signed-form AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("signed-form fields")
        .clone();
    signed_fields.push(Object::Reference(signature_field_id));
    signed_document
        .get_object_mut(acroform_id)
        .and_then(Object::as_dict_mut)
        .expect("mutable signed-form AcroForm")
        .set("Fields", signed_fields);
    signed_document
        .save(signed.as_path())
        .expect("save signed-form fixture");
    let signature_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"signed.pdf",
            "fields":[{"name":"profile.name","expected_value":"Alice","value":"Li Lei"}],
            "target_path":"signed-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("signed form fill must fail");
    assert!(signature_error
        .to_string()
        .contains("contain signature fields"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after rejected form fills"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_form_fill_rejects_ambiguous_radio_appearances_and_choice_indices() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_acroform_pdf(source.as_path());

    let duplicate_radio = root.join("duplicate-radio.pdf");
    fs::copy(source.as_path(), duplicate_radio.as_path()).expect("copy duplicate-radio fixture");
    let mut duplicate_radio_document =
        Document::load(duplicate_radio.as_path()).expect("load duplicate-radio PDF");
    let acroform_id = duplicate_radio_document
        .catalog()
        .expect("duplicate-radio catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("duplicate-radio AcroForm reference");
    let radio_id = duplicate_radio_document
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("duplicate-radio AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("duplicate-radio fields")[2]
        .as_reference()
        .expect("duplicate-radio field reference");
    let second_widget_id = duplicate_radio_document
        .get_object(radio_id)
        .and_then(Object::as_dict)
        .expect("duplicate-radio field")
        .get(b"Kids")
        .and_then(Object::as_array)
        .expect("duplicate-radio widgets")[1]
        .as_reference()
        .expect("duplicate-radio widget reference");
    let normal_appearance = duplicate_radio_document
        .get_object_mut(second_widget_id)
        .and_then(Object::as_dict_mut)
        .expect("duplicate-radio widget")
        .get_mut(b"AP")
        .and_then(Object::as_dict_mut)
        .expect("duplicate-radio AP")
        .get_mut(b"N")
        .and_then(Object::as_dict_mut)
        .expect("duplicate-radio AP/N");
    let premium = normal_appearance
        .remove(b"Premium")
        .expect("Premium appearance state");
    normal_appearance.set("Basic", premium);
    duplicate_radio_document
        .save(duplicate_radio.as_path())
        .expect("save duplicate-radio fixture");
    let duplicate_radio_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"duplicate-radio.pdf",
            "fields":[{"name":"subscription.plan","expected_value":"Basic","value":null}],
            "target_path":"duplicate-radio-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("ambiguous radio appearances must fail");
    assert!(duplicate_radio_error
        .to_string()
        .contains("unique non-Off appearance states"));

    let stale_choice = root.join("stale-choice-index.pdf");
    fs::copy(source.as_path(), stale_choice.as_path()).expect("copy stale-choice fixture");
    let mut stale_choice_document =
        Document::load(stale_choice.as_path()).expect("load stale-choice PDF");
    let acroform_id = stale_choice_document
        .catalog()
        .expect("stale-choice catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("stale-choice AcroForm reference");
    let choice_id = stale_choice_document
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("stale-choice AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("stale-choice fields")[3]
        .as_reference()
        .expect("stale-choice field reference");
    stale_choice_document
        .get_object_mut(choice_id)
        .and_then(Object::as_dict_mut)
        .expect("stale-choice field")
        .set("I", vec![Object::Integer(1)]);
    stale_choice_document
        .save(stale_choice.as_path())
        .expect("save stale-choice fixture");
    let stale_choice_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"stale-choice-index.pdf",
            "fields":[{"name":"profile.region","expected_value":"cn","value":"us"}],
            "target_path":"stale-choice-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("inconsistent choice V/I snapshot must fail");
    assert!(stale_choice_error
        .to_string()
        .contains("selected index does not match its selected value"));

    let stale_multi_choice = root.join("stale-multi-choice-index.pdf");
    fs::copy(source.as_path(), stale_multi_choice.as_path())
        .expect("copy stale multi-choice fixture");
    let mut stale_multi_choice_document =
        Document::load(stale_multi_choice.as_path()).expect("load stale multi-choice PDF");
    let acroform_id = stale_multi_choice_document
        .catalog()
        .expect("stale multi-choice catalog")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("stale multi-choice AcroForm reference");
    let choice_id = stale_multi_choice_document
        .get_object(acroform_id)
        .and_then(Object::as_dict)
        .expect("stale multi-choice AcroForm")
        .get(b"Fields")
        .and_then(Object::as_array)
        .expect("stale multi-choice fields")[5]
        .as_reference()
        .expect("stale multi-choice field reference");
    stale_multi_choice_document
        .get_object_mut(choice_id)
        .and_then(Object::as_dict_mut)
        .expect("stale multi-choice field")
        .set("I", vec![Object::Integer(0), Object::Integer(1)]);
    stale_multi_choice_document
        .save(stale_multi_choice.as_path())
        .expect("save stale multi-choice fixture");
    let stale_multi_choice_error = pdf_edit::fill_pdf_form_fields(
        &json!({
            "path":"stale-multi-choice-index.pdf",
            "fields":[{"name":"preferences.colors","expected_value":["red","blue"],"value":["green","blue"]}],
            "target_path":"stale-multi-choice-filled.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("inconsistent multi-choice V/I snapshot must fail");
    assert!(stale_multi_choice_error
        .to_string()
        .contains("indices do not match selected values"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn edits_pdf_pages_without_modifying_sources() {
    let (root, state, request) = test_context();
    let first = root.join("artifacts/first.pdf");
    let second = root.join("artifacts/second.pdf");
    write_blank_pdf(first.as_path(), 3);
    write_blank_pdf(second.as_path(), 2);
    let first_before = fs::read(first.as_path()).expect("first PDF bytes");

    let extracted = pdf_edit::extract_pdf_pages(
        &json!({
            "path":"artifacts/first.pdf",
            "pages":[1,3],
            "target_path":"artifacts/extracted.pdf"
        }),
        &state,
        &request,
    )
    .expect("extract pages");
    assert_eq!(extracted.get("page_count").and_then(Value::as_u64), Some(2));
    let extracted_document =
        Document::load(root.join("artifacts/extracted.pdf")).expect("extracted PDF");
    assert_eq!(extracted_document.get_pages().len(), 2);

    pdf_edit::rotate_pdf_pages(
        &json!({
            "path":"artifacts/first.pdf",
            "pages":[2],
            "angle":90,
            "target_path":"artifacts/rotated.pdf"
        }),
        &state,
        &request,
    )
    .expect("rotate pages");
    let rotated = Document::load(root.join("artifacts/rotated.pdf")).expect("rotated PDF");
    let page_id = rotated.get_pages().get(&2).copied().expect("page 2");
    let rotation = rotated
        .get_object(page_id)
        .and_then(Object::as_dict)
        .and_then(|page| page.get(b"Rotate"))
        .and_then(Object::as_i64)
        .expect("page rotation");
    assert_eq!(rotation, 90);

    let merged = pdf_edit::merge_pdfs(
        &json!({
            "paths":["artifacts/first.pdf","artifacts/second.pdf"],
            "target_path":"artifacts/merged.pdf"
        }),
        &state,
        &request,
    )
    .expect("merge PDFs");
    assert_eq!(merged.get("pages").and_then(Value::as_u64), Some(5));
    let merged_document = Document::load(root.join("artifacts/merged.pdf")).expect("merged PDF");
    assert_eq!(merged_document.get_pages().len(), 5);
    for page_id in merged_document.get_pages().into_values() {
        assert!(merged_document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .is_ok_and(|page| page.has(b"MediaBox")));
    }
    assert_eq!(
        fs::read(first.as_path()).expect("source bytes"),
        first_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn arranges_and_deletes_pdf_pages_in_the_exact_requested_order() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 4);
    let mut source_document = Document::load(source.as_path()).expect("source PDF");
    for (page_number, page_id) in source_document.get_pages() {
        let page = source_document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .expect("source page dictionary");
        page.set("Rotate", i64::from((page_number - 1) * 90));
    }
    source_document
        .save(source.as_path())
        .expect("save distinctive source PDF");
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    let arranged = pdf_edit::arrange_pdf_pages(
        &json!({
            "path":"artifacts/source.pdf",
            "pages":[4,2,1],
            "target_path":"artifacts/arranged.pdf"
        }),
        &state,
        &request,
    )
    .expect("arrange PDF pages");
    assert_eq!(
        arranged.get("operation").and_then(Value::as_str),
        Some("arrange_pages")
    );
    assert_eq!(arranged.get("pages"), Some(&json!([4, 2, 1])));
    assert_eq!(arranged.get("deleted_pages"), Some(&json!([3])));
    assert_eq!(
        arranged.get("source_page_count").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(arranged.get("page_count").and_then(Value::as_u64), Some(3));
    assert_eq!(
        arranged.get("reordered").and_then(Value::as_bool),
        Some(true)
    );

    let output = Document::load(root.join("artifacts/arranged.pdf")).expect("arranged PDF");
    let pages = output.get_pages();
    assert_eq!(pages.len(), 3);
    let rotations = pages
        .values()
        .map(|page_id| {
            output
                .get_object(*page_id)
                .and_then(Object::as_dict)
                .and_then(|page| page.get(b"Rotate"))
                .and_then(Object::as_i64)
                .expect("page rotation marker")
        })
        .collect::<Vec<_>>();
    assert_eq!(rotations, vec![270, 90, 0]);
    let root_id = output
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .expect("output pages root");
    let root_count = output
        .get_object(root_id)
        .and_then(Object::as_dict)
        .and_then(|dictionary| dictionary.get(b"Count"))
        .and_then(Object::as_i64)
        .expect("output page count");
    assert_eq!(root_count, 3);
    for page_id in pages.values() {
        let page = output
            .get_object(*page_id)
            .and_then(Object::as_dict)
            .expect("output page dictionary");
        assert_eq!(
            page.get(b"Parent")
                .and_then(Object::as_reference)
                .expect("output page parent"),
            root_id
        );
        assert!(page.has(b"MediaBox"));
    }
    assert_eq!(
        fs::read(source.as_path()).expect("source PDF after arrangement"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_page_arrangement_rejects_ambiguous_noop_complex_and_in_place_requests() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_blank_pdf(source.as_path(), 3);
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    for (index, arguments, expected) in [
        (
            0,
            json!({"path":"source.pdf","pages":[1,1],"target_path":"duplicate.pdf"}),
            "unique page numbers",
        ),
        (
            1,
            json!({"path":"source.pdf","pages":[4],"target_path":"outside.pdf"}),
            "outside 1..=3",
        ),
        (
            2,
            json!({"path":"source.pdf","pages":[1,2,3],"target_path":"noop.pdf"}),
            "change the page order",
        ),
        (
            3,
            json!({"path":"source.pdf","pages":[3,2,1],"target_path":"source.pdf","overwrite":true}),
            "distinct target_path",
        ),
    ] {
        let error = pdf_edit::arrange_pdf_pages(&arguments, &state, &request)
            .expect_err("unsafe page arrangement must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    let complex = root.join("complex.pdf");
    write_blank_pdf(complex.as_path(), 2);
    let mut complex_document = Document::load(complex.as_path()).expect("complex PDF");
    complex_document
        .catalog_mut()
        .expect("complex catalog")
        .set("PageLabels", dictionary! { "Nums" => Vec::<Object>::new() });
    complex_document
        .save(complex.as_path())
        .expect("save complex PDF");
    let complex_error = pdf_edit::arrange_pdf_pages(
        &json!({"path":"complex.pdf","pages":[2,1],"target_path":"complex-output.pdf"}),
        &state,
        &request,
    )
    .expect_err("page labels must fail closed");
    assert!(complex_error.to_string().contains("/PageLabels"));

    assert_eq!(
        fs::read(source.as_path()).expect("source PDF after rejections"),
        source_before
    );
    for target in [
        "duplicate.pdf",
        "outside.pdf",
        "noop.pdf",
        "complex-output.pdf",
    ] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stamps_selected_pdf_pages_without_modifying_the_source() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 3);
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    let stamped = pdf_edit::stamp_pdf_text(
        &json!({
            "path":"artifacts/source.pdf",
            "text":"CONFIDENTIAL",
            "pages":[2],
            "position":"top_right",
            "font_size":24,
            "margin_points":36,
            "rotation":-45,
            "opacity":0.2,
            "grayscale":0.35,
            "target_path":"artifacts/stamped.pdf"
        }),
        &state,
        &request,
    )
    .expect("stamp selected PDF page");
    assert_eq!(
        stamped.get("operation").and_then(Value::as_str),
        Some("stamp_text")
    );
    assert_eq!(stamped.get("pages"), Some(&json!([2])));
    assert_eq!(stamped.get("rotation").and_then(Value::as_i64), Some(-45));

    let document = Document::load(root.join("artifacts/stamped.pdf")).expect("stamped PDF");
    assert_eq!(document.get_pages().len(), 3);
    let page_two = document.get_pages().get(&2).copied().expect("page two");
    let page_two_dictionary = document
        .get_object(page_two)
        .and_then(Object::as_dict)
        .expect("stamped page dictionary");
    let resources = page_two_dictionary
        .get(b"Resources")
        .and_then(Object::as_dict)
        .expect("materialized stamp resources");
    assert!(resources.has(b"Font"));
    assert!(resources.has(b"ExtGState"));
    assert!(document
        .extract_text(&[2])
        .expect("extract stamped text")
        .contains("CONFIDENTIAL"));
    assert!(!document
        .extract_text(&[1])
        .expect("extract unstamped page text")
        .contains("CONFIDENTIAL"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after stamp"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stamps_dynamic_pdf_page_numbers_from_physical_positions_without_modifying_source() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 4);
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    let stamped = pdf_edit::stamp_pdf_page_numbers(
        &json!({
            "path":"artifacts/source.pdf",
            "pages":[2,4],
            "format":"page_number_of_total",
            "start_number":5,
            "position":"bottom_center",
            "font_size":10,
            "margin_points":24,
            "opacity":0.9,
            "grayscale":0.1,
            "target_path":"artifacts/numbered.pdf"
        }),
        &state,
        &request,
    )
    .expect("stamp dynamic PDF page numbers");
    assert_eq!(
        stamped.get("operation").and_then(Value::as_str),
        Some("stamp_page_numbers")
    );
    assert_eq!(stamped.get("pages"), Some(&json!([2, 4])));
    assert_eq!(
        stamped.get("first_label").and_then(Value::as_str),
        Some("Page 6 of 8")
    );
    assert_eq!(
        stamped.get("last_label").and_then(Value::as_str),
        Some("Page 8 of 8")
    );

    let document = Document::load(root.join("artifacts/numbered.pdf")).expect("numbered PDF");
    assert_eq!(document.get_pages().len(), 4);
    assert!(!document
        .extract_text(&[1])
        .expect("extract page one")
        .contains("Page"));
    assert!(document
        .extract_text(&[2])
        .expect("extract page two")
        .contains("Page 6 of 8"));
    assert!(!document
        .extract_text(&[3])
        .expect("extract page three")
        .contains("Page"));
    assert!(document
        .extract_text(&[4])
        .expect("extract page four")
        .contains("Page 8 of 8"));
    for page_number in [2, 4] {
        let page_id = document
            .get_pages()
            .get(&page_number)
            .copied()
            .expect("stamped page");
        let resources = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .and_then(|page| page.get(b"Resources"))
            .and_then(Object::as_dict)
            .expect("materialized page-number resources");
        assert!(resources.has(b"Font"));
        assert!(resources.has(b"ExtGState"));
    }
    assert_eq!(
        fs::read(source.as_path()).expect("source after numbering"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_page_numbering_rejects_invalid_format_overflow_order_position_and_in_place() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_blank_pdf(source.as_path(), 3);
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");
    for (target, arguments, expected) in [
        (
            "format.pdf",
            json!({"format":"roman"}),
            "supported PDF page-number format",
        ),
        (
            "overflow.pdf",
            json!({"start_number":999999}),
            "would exceed 1000000",
        ),
        ("order.pdf", json!({"pages":[3,1]}), "ascending order"),
        (
            "position.pdf",
            json!({"position":"center"}),
            "page-number position",
        ),
    ] {
        let mut request_arguments = arguments;
        request_arguments["path"] = json!("source.pdf");
        request_arguments["target_path"] = json!(target);
        let error = pdf_edit::stamp_pdf_page_numbers(&request_arguments, &state, &request)
            .expect_err("invalid page numbering request must fail");
        assert!(error.to_string().contains(expected));
        assert!(!root.join(target).exists());
    }

    let in_place = pdf_edit::stamp_pdf_page_numbers(
        &json!({
            "path":"source.pdf",
            "target_path":"source.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("in-place page numbering must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after failures"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn adds_and_inspects_unicode_pdf_text_annotation_without_modifying_source() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 3);
    let mut prepared = Document::load(source.as_path()).expect("source PDF");
    let page_id = prepared.get_pages().get(&2).copied().expect("page two");
    let existing_id = prepared.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => vec![36.into(), 36.into(), 60.into(), 60.into()],
        "Contents" => lopdf::text_string("Existing note"),
        "Name" => "Note",
        "Open" => false,
        "P" => page_id,
    });
    let annotations_id = prepared.add_object(vec![Object::Reference(existing_id)]);
    prepared
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .expect("page dictionary")
        .set("Annots", annotations_id);
    prepared.save(source.as_path()).expect("save source PDF");
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");

    let added = pdf_edit::add_pdf_text_annotation(
        &json!({
            "path":"artifacts/source.pdf",
            "page":2,
            "text":"请复核第二页\n金额与合同不一致。",
            "author":"李雷",
            "position":"top_right",
            "icon":"comment",
            "color":"blue",
            "size_points":32,
            "margin_points":24,
            "open":true,
            "target_path":"artifacts/annotated.pdf"
        }),
        &state,
        &request,
    )
    .expect("add PDF Text annotation");
    assert_eq!(
        added.get("operation").and_then(Value::as_str),
        Some("add_text_annotation")
    );
    assert_eq!(added.get("page").and_then(Value::as_u64), Some(2));
    assert_eq!(added.get("icon").and_then(Value::as_str), Some("comment"));

    let document = Document::load(root.join("artifacts/annotated.pdf")).expect("annotated PDF");
    let page_id = document
        .get_pages()
        .get(&2)
        .copied()
        .expect("annotated page");
    let page = document
        .get_object(page_id)
        .and_then(Object::as_dict)
        .expect("annotated page dictionary");
    let annotations = page
        .get(b"Annots")
        .and_then(Object::as_array)
        .expect("annotation array");
    assert_eq!(annotations.len(), 2);
    let dictionaries = annotations
        .iter()
        .map(|annotation| {
            let annotation_id = annotation.as_reference().expect("annotation reference");
            document
                .get_object(annotation_id)
                .and_then(Object::as_dict)
                .expect("annotation dictionary")
        })
        .collect::<Vec<_>>();
    assert!(dictionaries.iter().any(|annotation| {
        annotation
            .get(b"Contents")
            .ok()
            .and_then(|value| lopdf::decode_text_string(value).ok())
            .as_deref()
            == Some("Existing note")
    }));
    let annotation = dictionaries
        .iter()
        .copied()
        .find(|annotation| {
            annotation
                .get(b"Contents")
                .ok()
                .and_then(|value| lopdf::decode_text_string(value).ok())
                .as_deref()
                == Some("请复核第二页\n金额与合同不一致。")
        })
        .expect("new annotation");
    assert_eq!(
        annotation
            .get(b"Subtype")
            .and_then(Object::as_name)
            .expect("annotation subtype"),
        b"Text"
    );
    assert_eq!(
        annotation
            .get(b"Name")
            .and_then(Object::as_name)
            .expect("annotation icon"),
        b"Comment"
    );
    assert!(annotation
        .get(b"Open")
        .and_then(Object::as_bool)
        .expect("annotation open state"));
    assert_eq!(
        annotation
            .get(b"F")
            .and_then(Object::as_i64)
            .expect("annotation flags"),
        4
    );
    assert_eq!(
        annotation
            .get(b"T")
            .map(lopdf::decode_text_string)
            .expect("author string")
            .expect("decode author"),
        "李雷"
    );
    assert_eq!(
        annotation
            .get(b"P")
            .and_then(Object::as_reference)
            .expect("annotation page reference"),
        page_id
    );
    let rect = annotation
        .get(b"Rect")
        .and_then(Object::as_array)
        .expect("annotation rect")
        .iter()
        .map(|value| value.as_float().expect("rect number"))
        .collect::<Vec<_>>();
    assert_eq!(rect, vec![539.0, 786.0, 571.0, 818.0]);

    let inspected = inspect_pdf(&json!({"path":"artifacts/annotated.pdf"}), &state, &request)
        .expect("inspect annotations");
    let summary = inspected.get("annotations").expect("annotation summary");
    assert_eq!(summary.get("count").and_then(Value::as_u64), Some(2));
    assert_eq!(summary.get("text_count").and_then(Value::as_u64), Some(2));
    assert!(summary
        .get("preview")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|item| {
            item.get("contents").and_then(Value::as_str) == Some("请复核第二页\n金额与合同不一致。")
                && item.get("author").and_then(Value::as_str) == Some("李雷")
        })));
    assert_eq!(
        fs::read(source.as_path()).expect("source after annotation"),
        source_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_text_annotation_rejects_invalid_inputs_rotation_malformed_annots_and_in_place() {
    let (root, state, request) = test_context();
    let source = root.join("source.pdf");
    write_blank_pdf(source.as_path(), 2);
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");
    for (target, arguments, expected) in [
        ("page.pdf", json!({"page":3}), "page 3 does not exist"),
        (
            "control.pdf",
            json!({"page":1,"text":"bad\u{0000}text"}),
            "unsupported control character",
        ),
        (
            "author.pdf",
            json!({"page":1,"author":"bad\nauthor"}),
            "unsupported control character",
        ),
        (
            "position.pdf",
            json!({"page":1,"position":"center"}),
            "text-annotation position",
        ),
        (
            "icon.pdf",
            json!({"page":1,"icon":"push_pin"}),
            "Text annotation icon",
        ),
        (
            "color.pdf",
            json!({"page":1,"color":"purple"}),
            "annotation color",
        ),
        (
            "size.pdf",
            json!({"page":1,"size_points":100}),
            "size_points must be between 12 and 72",
        ),
    ] {
        let mut request_arguments = arguments;
        request_arguments["path"] = json!("source.pdf");
        request_arguments["text"] = request_arguments
            .get("text")
            .cloned()
            .unwrap_or_else(|| json!("Review this page"));
        request_arguments["target_path"] = json!(target);
        let error = pdf_edit::add_pdf_text_annotation(&request_arguments, &state, &request)
            .expect_err("invalid annotation request must fail");
        assert!(
            error.to_string().contains(expected),
            "unexpected error for {target}: {error}"
        );
        assert!(!root.join(target).exists());
    }

    let in_place = pdf_edit::add_pdf_text_annotation(
        &json!({
            "path":"source.pdf",
            "page":1,
            "text":"Review this page",
            "target_path":"source.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("in-place annotation must fail");
    assert!(in_place.to_string().contains("distinct target_path"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after invalid requests"),
        source_before
    );

    let rotated = root.join("rotated.pdf");
    write_blank_pdf(rotated.as_path(), 1);
    let mut document = Document::load(rotated.as_path()).expect("rotated source");
    let page_id = document.get_pages()[&1];
    document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .expect("rotated page")
        .set("Rotate", 90);
    document.save(rotated.as_path()).expect("save rotated PDF");
    let rotation_error = pdf_edit::add_pdf_text_annotation(
        &json!({
            "path":"rotated.pdf",
            "page":1,
            "text":"Review this page",
            "target_path":"rotated-output.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("rotated annotation must fail");
    assert!(rotation_error
        .to_string()
        .contains("requires an unrotated page"));
    assert!(!root.join("rotated-output.pdf").exists());

    let malformed = root.join("malformed.pdf");
    write_blank_pdf(malformed.as_path(), 1);
    let mut document = Document::load(malformed.as_path()).expect("malformed source");
    let page_id = document.get_pages()[&1];
    document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .expect("malformed page")
        .set("Annots", 7);
    document
        .save(malformed.as_path())
        .expect("save malformed PDF");
    let malformed_error = pdf_edit::add_pdf_text_annotation(
        &json!({
            "path":"malformed.pdf",
            "page":1,
            "text":"Review this page",
            "target_path":"malformed-output.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("malformed annotation array must fail");
    assert!(malformed_error.to_string().contains("must be an array"));
    assert!(!root.join("malformed-output.pdf").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stamps_alpha_png_on_selected_pdf_pages_without_modifying_sources() {
    let (root, state, request) = test_context();
    let source = root.join("artifacts/source.pdf");
    write_blank_pdf(source.as_path(), 3);
    fs::create_dir_all(root.join("assets")).expect("assets");
    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::write(root.join("assets/signature.png"), image).expect("write PNG");
    let source_before = fs::read(source.as_path()).expect("source PDF bytes");
    let image_before = fs::read(root.join("assets/signature.png")).expect("source image bytes");

    let stamped = pdf_edit::stamp_pdf_image(
        &json!({
            "path":"artifacts/source.pdf",
            "image_path":"assets/signature.png",
            "pages":[2],
            "position":"bottom_right",
            "width_points":72,
            "margin_points":24,
            "rotation":0,
            "opacity":0.8,
            "target_path":"artifacts/signed.pdf"
        }),
        &state,
        &request,
    )
    .expect("stamp PDF image");
    assert_eq!(
        stamped.get("operation").and_then(Value::as_str),
        Some("stamp_image")
    );
    assert_eq!(stamped.get("pages"), Some(&json!([2])));
    assert_eq!(
        stamped.get("image_format").and_then(Value::as_str),
        Some("png")
    );
    assert_eq!(
        stamped.get("image_width_pixels").and_then(Value::as_u64),
        Some(1)
    );

    let document = Document::load(root.join("artifacts/signed.pdf")).expect("signed PDF");
    let page_two = document.get_pages().get(&2).copied().expect("page two");
    let page_two_dictionary = document
        .get_object(page_two)
        .and_then(Object::as_dict)
        .expect("stamped page dictionary");
    let resources = page_two_dictionary
        .get(b"Resources")
        .and_then(Object::as_dict)
        .expect("materialized image resources");
    let xobjects = resources
        .get(b"XObject")
        .and_then(Object::as_dict)
        .expect("image XObject resources");
    assert_eq!(xobjects.len(), 1);
    let image_id = xobjects
        .iter()
        .next()
        .and_then(|(_, value)| value.as_reference().ok())
        .expect("image reference");
    let image_stream = document
        .get_object(image_id)
        .and_then(Object::as_stream)
        .expect("image stream");
    assert_eq!(
        image_stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .expect("image subtype"),
        b"Image"
    );
    assert!(image_stream.dict.has(b"SMask"));
    assert!(resources.has(b"ExtGState"));
    let page_one = document.get_pages().get(&1).copied().expect("page one");
    assert!(!document
        .get_object(page_one)
        .and_then(Object::as_dict)
        .is_ok_and(|page| page.has(b"Resources")));
    assert_eq!(
        fs::read(source.as_path()).expect("source after stamp"),
        source_before
    );
    assert_eq!(
        fs::read(root.join("assets/signature.png")).expect("image after stamp"),
        image_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_image_stamping_rejects_invalid_images_pages_symlinks_and_in_place_output() {
    let (root, state, request) = test_context();
    write_blank_pdf(root.join("source.pdf").as_path(), 2);
    fs::write(root.join("invalid.png"), b"not a png").expect("invalid PNG");
    let valid_image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("test PNG");
    fs::write(root.join("valid.png"), valid_image).expect("valid PNG");
    let minimal_jpeg = vec![
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x01, 0x00, 0x01, 0x03, 0x01, 0x11, 0x00,
        0x02, 0x11, 0x00, 0x03, 0x11, 0x00, 0xff, 0xda, 0x00, 0x0c, 0x03, 0x01, 0x00, 0x02, 0x11,
        0x03, 0x11, 0x00, 0x3f, 0x00, 0x00, 0xff, 0xd9,
    ];
    fs::write(root.join("minimal.jpg"), minimal_jpeg).expect("minimal JPEG");
    let jpeg_stamped = pdf_edit::stamp_pdf_image(
        &json!({
            "path":"source.pdf",
            "image_path":"minimal.jpg",
            "pages":[1],
            "target_path":"jpeg-output.pdf"
        }),
        &state,
        &request,
    )
    .expect("JPEG stamp contract");
    assert_eq!(
        jpeg_stamped.get("image_format").and_then(Value::as_str),
        Some("jpeg")
    );
    assert!(root.join("jpeg-output.pdf").is_file());

    for (index, arguments, expected) in [
        (
            0,
            json!({
                "path":"source.pdf",
                "image_path":"invalid.png",
                "target_path":"invalid-output.pdf"
            }),
            "PNG image",
        ),
        (
            1,
            json!({
                "path":"source.pdf",
                "image_path":"valid.png",
                "pages":[2,1],
                "target_path":"unsorted.pdf"
            }),
            "ascending order",
        ),
        (
            2,
            json!({
                "path":"source.pdf",
                "image_path":"valid.png",
                "target_path":"source.pdf",
                "overwrite":true
            }),
            "distinct target_path",
        ),
    ] {
        let error = pdf_edit::stamp_pdf_image(&arguments, &state, &request)
            .expect_err("unsafe PDF image stamp must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("valid.png"), root.join("linked.png"))
            .expect("image symlink");
        let error = pdf_edit::stamp_pdf_image(
            &json!({
                "path":"source.pdf",
                "image_path":"linked.png",
                "target_path":"linked-output.pdf"
            }),
            &state,
            &request,
        )
        .expect_err("symlink image must fail");
        assert!(error.to_string().contains("non-symlink"));
    }
    for target in ["invalid-output.pdf", "unsorted.pdf", "linked-output.pdf"] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_text_stamping_rejects_unsafe_text_pages_and_in_place_output() {
    let (root, state, request) = test_context();
    write_blank_pdf(root.join("source.pdf").as_path(), 2);
    for (index, arguments, expected) in [
        (
            0,
            json!({
                "path":"source.pdf",
                "text":"中文",
                "target_path":"unicode.pdf"
            }),
            "printable ASCII",
        ),
        (
            1,
            json!({
                "path":"source.pdf",
                "text":"two\nlines",
                "target_path":"multiline.pdf"
            }),
            "single line",
        ),
        (
            2,
            json!({
                "path":"source.pdf",
                "text":"STAMP",
                "pages":[2,1],
                "target_path":"unsorted.pdf"
            }),
            "ascending order",
        ),
        (
            3,
            json!({
                "path":"source.pdf",
                "text":"STAMP",
                "target_path":"source.pdf",
                "overwrite":true
            }),
            "distinct target_path",
        ),
    ] {
        let error = pdf_edit::stamp_pdf_text(&arguments, &state, &request)
            .expect_err("unsafe PDF stamp must fail");
        assert!(
            error.to_string().contains(expected),
            "case {index}: {error:#}"
        );
    }
    for target in ["unicode.pdf", "multiline.pdf", "unsorted.pdf"] {
        assert!(!root.join(target).exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pdf_edits_reject_in_place_and_ambiguous_page_selection() {
    let (root, state, request) = test_context();
    write_blank_pdf(root.join("source.pdf").as_path(), 3);

    let in_place = pdf_edit::rotate_pdf_pages(
        &json!({
            "path":"source.pdf",
            "angle":90,
            "target_path":"source.pdf",
            "overwrite":true
        }),
        &state,
        &request,
    )
    .expect_err("in-place edit must fail");
    assert!(in_place.to_string().contains("distinct target_path"));

    let unsorted = pdf_edit::extract_pdf_pages(
        &json!({
            "path":"source.pdf",
            "pages":[2,1],
            "target_path":"output.pdf"
        }),
        &state,
        &request,
    )
    .expect_err("unsorted pages must fail");
    assert!(unsorted.to_string().contains("ascending order"));
    assert!(!root.join("output.pdf").exists());
    let _ = fs::remove_dir_all(root);
}
