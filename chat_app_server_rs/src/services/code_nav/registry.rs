use std::sync::Arc;

use super::languages::c::CCodeNavProvider;
use super::languages::cpp::CppCodeNavProvider;
use super::languages::csharp::CSharpCodeNavProvider;
use super::languages::go::GoCodeNavProvider;
use super::languages::java::JavaCodeNavProvider;
use super::languages::javascript::JavaScriptCodeNavProvider;
use super::languages::kotlin::KotlinCodeNavProvider;
use super::languages::python::PythonCodeNavProvider;
use super::languages::rust::RustCodeNavProvider;
use super::languages::typescript::TypeScriptCodeNavProvider;
use super::CodeNavProvider;

pub fn default_providers() -> Vec<Arc<dyn CodeNavProvider>> {
    vec![
        Arc::new(JavaCodeNavProvider),
        Arc::new(KotlinCodeNavProvider),
        Arc::new(TypeScriptCodeNavProvider),
        Arc::new(JavaScriptCodeNavProvider),
        Arc::new(CppCodeNavProvider),
        Arc::new(CCodeNavProvider),
        Arc::new(CSharpCodeNavProvider),
        Arc::new(RustCodeNavProvider),
        Arc::new(GoCodeNavProvider),
        Arc::new(PythonCodeNavProvider),
    ]
}
