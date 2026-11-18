//! Safe wrapper around `libsqlite3-sys` with WAL mode and foreign keys enabled.

use libsqlite3_sys as ffi;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr;

#[derive(Debug, Clone)]
pub enum SqlParam {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

impl From<i64> for SqlParam {
    fn from(v: i64) -> Self {
        SqlParam::Int(v)
    }
}

impl From<i32> for SqlParam {
    fn from(v: i32) -> Self {
        SqlParam::Int(v as i64)
    }
}

impl From<f64> for SqlParam {
    fn from(v: f64) -> Self {
        SqlParam::Float(v)
    }
}

impl From<&str> for SqlParam {
    fn from(v: &str) -> Self {
        SqlParam::Text(v.to_string())
    }
}

impl From<String> for SqlParam {
    fn from(v: String) -> Self {
        SqlParam::Text(v)
    }
}

impl<T: Into<SqlParam>> From<Option<T>> for SqlParam {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => SqlParam::Null,
        }
    }
}

pub struct SqliteConn {
    db: *mut ffi::sqlite3,
}

impl SqliteConn {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let path_str = path.to_string_lossy();
        let c_path = CString::new(path_str.as_bytes()).map_err(|e| e.to_string())?;
        let mut db: *mut ffi::sqlite3 = ptr::null_mut();
        let flags = ffi::SQLITE_OPEN_READWRITE
            | ffi::SQLITE_OPEN_CREATE
            | ffi::SQLITE_OPEN_FULLMUTEX;

        let rc = unsafe { ffi::sqlite3_open_v2(c_path.as_ptr(), &mut db, flags, ptr::null()) };
        if rc != ffi::SQLITE_OK {
            let err = Self::err_msg_raw(db);
            if !db.is_null() {
                unsafe {
                    ffi::sqlite3_close(db);
                }
            }
            return Err(format!("Failed to open SQLite database: {err}"));
        }

        unsafe {
            ffi::sqlite3_busy_timeout(db, 5000);
        }

        let conn = Self { db };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(conn)
    }

    fn err_msg_raw(db: *mut ffi::sqlite3) -> String {
        if db.is_null() {
            return "Unknown SQLite error".to_string();
        }
        unsafe {
            let ptr = ffi::sqlite3_errmsg(db);
            if ptr.is_null() {
                "Unknown SQLite error".to_string()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    fn err_msg(&self) -> String {
        Self::err_msg_raw(self.db)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<(), String> {
        let c_sql = CString::new(sql).map_err(|e| e.to_string())?;
        let mut err_msg: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_exec(
                self.db,
                c_sql.as_ptr(),
                None,
                ptr::null_mut(),
                &mut err_msg,
            )
        };
        if rc != ffi::SQLITE_OK {
            let msg = if !err_msg.is_null() {
                let s = unsafe { CStr::from_ptr(err_msg).to_string_lossy().into_owned() };
                unsafe {
                    ffi::sqlite3_free(err_msg as *mut _);
                }
                s
            } else {
                self.err_msg()
            };
            return Err(msg);
        }
        Ok(())
    }

    unsafe fn bind_params(stmt: *mut ffi::sqlite3_stmt, params: &[SqlParam]) -> Result<(), String> {
        for (idx, param) in params.iter().enumerate() {
            let pos = (idx + 1) as c_int;
            let rc = match param {
                SqlParam::Null => ffi::sqlite3_bind_null(stmt, pos),
                SqlParam::Int(v) => ffi::sqlite3_bind_int64(stmt, pos, *v),
                SqlParam::Float(v) => ffi::sqlite3_bind_double(stmt, pos, *v),
                SqlParam::Text(s) => {
                    let bytes = s.as_bytes();
                    ffi::sqlite3_bind_text(
                        stmt,
                        pos,
                        bytes.as_ptr() as *const c_char,
                        bytes.len() as c_int,
                        ffi::SQLITE_TRANSIENT(),
                    )
                }
            };
            if rc != ffi::SQLITE_OK {
                return Err(format!("Failed to bind SQLite parameter at index {pos}"));
            }
        }
        Ok(())
    }

    pub fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<usize, String> {
        let c_sql = CString::new(sql).map_err(|e| e.to_string())?;
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_prepare_v2(
                self.db,
                c_sql.as_ptr(),
                -1,
                &mut stmt,
                ptr::null_mut(),
            )
        };
        if rc != ffi::SQLITE_OK {
            return Err(self.err_msg());
        }

        let res = unsafe {
            if let Err(e) = Self::bind_params(stmt, params) {
                ffi::sqlite3_finalize(stmt);
                return Err(e);
            }
            let step_rc = ffi::sqlite3_step(stmt);
            if step_rc != ffi::SQLITE_DONE && step_rc != ffi::SQLITE_ROW {
                let err = self.err_msg();
                ffi::sqlite3_finalize(stmt);
                return Err(err);
            }
            let changes = ffi::sqlite3_changes(self.db) as usize;
            ffi::sqlite3_finalize(stmt);
            Ok(changes)
        };
        res
    }

    pub fn insert(&self, sql: &str, params: &[SqlParam]) -> Result<i64, String> {
        self.execute(sql, params)?;
        let rowid = unsafe { ffi::sqlite3_last_insert_rowid(self.db) };
        Ok(rowid)
    }

    pub fn query_rows(
        &self,
        sql: &str,
        params: &[SqlParam],
    ) -> Result<Vec<HashMap<String, Value>>, String> {
        let c_sql = CString::new(sql).map_err(|e| e.to_string())?;
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_prepare_v2(
                self.db,
                c_sql.as_ptr(),
                -1,
                &mut stmt,
                ptr::null_mut(),
            )
        };
        if rc != ffi::SQLITE_OK {
            return Err(self.err_msg());
        }

        unsafe {
            if let Err(e) = Self::bind_params(stmt, params) {
                ffi::sqlite3_finalize(stmt);
                return Err(e);
            }

            let col_count = ffi::sqlite3_column_count(stmt);
            let mut col_names = Vec::with_capacity(col_count as usize);
            for i in 0..col_count {
                let name_ptr = ffi::sqlite3_column_name(stmt, i);
                let name = if name_ptr.is_null() {
                    format!("col_{i}")
                } else {
                    CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
                };
                col_names.push(name);
            }

            let mut rows = Vec::new();
            loop {
                let step_rc = ffi::sqlite3_step(stmt);
                if step_rc == ffi::SQLITE_ROW {
                    let mut map = HashMap::with_capacity(col_count as usize);
                    for (i, col_name) in col_names.iter().enumerate() {
                        let col_idx = i as c_int;
                        let col_type = ffi::sqlite3_column_type(stmt, col_idx);
                        let val = match col_type {
                            ffi::SQLITE_INTEGER => {
                                let v = ffi::sqlite3_column_int64(stmt, col_idx);
                                json!(v)
                            }
                            ffi::SQLITE_FLOAT => {
                                let v = ffi::sqlite3_column_double(stmt, col_idx);
                                json!(v)
                            }
                            ffi::SQLITE_TEXT => {
                                let ptr = ffi::sqlite3_column_text(stmt, col_idx);
                                let len = ffi::sqlite3_column_bytes(stmt, col_idx);
                                if ptr.is_null() || len <= 0 {
                                    json!("")
                                } else {
                                    let slice =
                                        std::slice::from_raw_parts(ptr, len as usize);
                                    let s = String::from_utf8_lossy(slice).into_owned();
                                    json!(s)
                                }
                            }
                            _ => Value::Null,
                        };
                        map.insert(col_name.clone(), val);
                    }
                    rows.push(map);
                } else if step_rc == ffi::SQLITE_DONE {
                    break;
                } else {
                    let err = self.err_msg();
                    ffi::sqlite3_finalize(stmt);
                    return Err(err);
                }
            }

            ffi::sqlite3_finalize(stmt);
            Ok(rows)
        }
    }
}

impl Drop for SqliteConn {
    fn drop(&mut self) {
        if !self.db.is_null() {
            unsafe {
                ffi::sqlite3_close(self.db);
            }
            self.db = ptr::null_mut();
        }
    }
}
