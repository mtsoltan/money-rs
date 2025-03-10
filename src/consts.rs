use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;

pub(crate) const EPSILON: f64 = 1e-5f64;

pub type Pool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;

use diesel::backend::Backend;
use diesel::connection::{AnsiTransactionManager, TransactionManager};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::{AstPass, QueryBuilder, QueryFragment};
use diesel::result::Error;

pub struct TransactionBuilder<'a, C> {
    pub connection: &'a mut C,
    isolation_level: Option<IsolationLevel>,
    read_mode: Option<ReadMode>,
    deferrable: Option<Deferrable>,
}

impl<'a, C> TransactionBuilder<'a, C>
where
    C: Connection<Backend = Pg, TransactionManager = AnsiTransactionManager>,
{
    pub(crate) fn new(connection: &'a mut C) -> Self {
        Self {
            connection,
            isolation_level: None,
            read_mode: None,
            deferrable: None,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_mode = Some(ReadMode::ReadOnly);
        self
    }

    pub fn read_write(mut self) -> Self {
        self.read_mode = Some(ReadMode::ReadWrite);
        self
    }

    pub fn deferrable(mut self) -> Self {
        self.deferrable = Some(Deferrable::Deferrable);
        self
    }

    pub fn not_deferrable(mut self) -> Self {
        self.deferrable = Some(Deferrable::NotDeferrable);
        self
    }

    pub fn read_committed(mut self) -> Self {
        self.isolation_level = Some(IsolationLevel::ReadCommitted);
        self
    }

    pub fn repeatable_read(mut self) -> Self {
        self.isolation_level = Some(IsolationLevel::RepeatableRead);
        self
    }

    pub fn serializable(mut self) -> Self {
        self.isolation_level = Some(IsolationLevel::Serializable);
        self
    }

    pub fn start_transaction(&mut self) ->  QueryResult<()> {
        let mut query_builder = <Pg as Backend>::QueryBuilder::default();
        self.to_sql(&mut query_builder, &Pg)?;
        let sql = query_builder.finish();

        AnsiTransactionManager::begin_transaction_sql(&mut *self.connection, &sql)
    }

    pub fn commit_transaction(&mut self) -> QueryResult<()> {
        AnsiTransactionManager::commit_transaction(&mut *self.connection)
    }
    pub fn rollback_transaction(&mut self) -> QueryResult<()> {
        AnsiTransactionManager::rollback_transaction(&mut *self.connection)
    }
}

impl<C> QueryFragment<Pg> for TransactionBuilder<'_, C> {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("BEGIN TRANSACTION");
        if let Some(ref isolation_level) = self.isolation_level {
            isolation_level.walk_ast(out.reborrow())?;
        }
        if let Some(ref read_mode) = self.read_mode {
            read_mode.walk_ast(out.reborrow())?;
        }
        if let Some(ref deferrable) = self.deferrable {
            deferrable.walk_ast(out.reborrow())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum IsolationLevel {
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl QueryFragment<Pg> for IsolationLevel {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql(" ISOLATION LEVEL ");
        match *self {
            IsolationLevel::ReadCommitted => out.push_sql("READ COMMITTED"),
            IsolationLevel::RepeatableRead => out.push_sql("REPEATABLE READ"),
            IsolationLevel::Serializable => out.push_sql("SERIALIZABLE"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum ReadMode {
    ReadOnly,
    ReadWrite,
}

impl QueryFragment<Pg> for ReadMode {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        match *self {
            ReadMode::ReadOnly => out.push_sql(" READ ONLY"),
            ReadMode::ReadWrite => out.push_sql(" READ WRITE"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Deferrable {
    Deferrable,
    NotDeferrable,
}

impl QueryFragment<Pg> for Deferrable {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        match *self {
            Deferrable::Deferrable => out.push_sql(" DEFERRABLE"),
            Deferrable::NotDeferrable => out.push_sql(" NOT DEFERRABLE"),
        }
        Ok(())
    }
}
