
Contributions draft:
- TL;DR: If you follow the structure and style of existing code, you should be fine for most part.
- DOIT: No unwraps
- DOIT: Vec::new over vec![]
- DOIT: Prefer generics on functions when needed rather than on the variable binding
- DOIT: For use vs. qualify: If it's going to be used less than thrice, qualify it. If it's going to be used >= thrice, use.
- Exception is for things like "Error" or "Result" from different crates, which should always be qualified regardless.
- If something is there just for usage in a test, always wrap it in the cfg for test.
- No dbg!()s left uncommented in any committed code. If you're done debugging, you can either remove it (if you won't need it agian), or comment it (if you'll need it again).
- No littering handlers with logs. Instead of logging to verify the behavior of your code, use tests. In most part, this codebase only logs internal server errors.
- Prefer descriptive names over one-letter names in variables. However, use one-letter names in match arms, iter function args or other short-lived well-scoped binds.
- Matching and remapping errors vs. using question-mark and Into traits are both possible in this codebase. Prefer matching and remapping for patterns that are less frequently encountered, and question-mark and traits for more frequently encountered patterns.
- All to-do's in this repo should have some `(NN): ` after the uppercase word, specifying their priority. Lower numbers are higher priority.
- TODO(90): clippy allows
- TODO(90): function, trait, and struct documentation
- TODO(90): Creation of macros
- TODO(90): Library usage
- TODO(90): Error types, error usage, error handling, usage of expect
- The less files the better. If you'll contribute something that may belong to an existing file, don't create a new file for it.
- Flat structure, no suborganization of packages or anything like that.
- Layers are simple: Router -> Handler -> Model, with logic always belonging to one of the latter two, or to a utiltiy.
- Rather important core logic that needs a clear separation in any diff (numeric.rs, authentication.rs) can be separated in its own file isntead of remain in model.rs, handlers.rs respectively.
- Use fpdec for all decimal calculations. Do not use moneta as currencies are user specified.
- Use diesel (with diesel-async traits) for all database interaction.
- Do not write any raw queries
- Responses, create requests, update requests, database create DTOs, and database update DTOs should never be specified manually, but should use inner_macros.
- Requests and responses for things other than direct entity CURD can be written in the handlers.
- Every endpoint should have happy path covered by the main.rs integration test
- Sad paths for rather complex interacting endpoints should be covered by integration tests.
- Complex mathemtical functionality should _fully_ be covered by unit tests.
- We use comments in the code to explain complex usage. In logic that has many steps, we prefer comments with number-points.
- We write complex logic in one function, and do not split it between functions. It's higher cyclomatic complexity but easier to keep track of in one context in your head, reducing context switching and cross-context related logic bugs.
- Routes with query parameters have their GET query parameters specified in a commment in the router in main.rs
- We format using cargo fmt, all committed code should be formatted.
- We write SQL migrations for diesel. All migrations should have a `down` that restores both _data_ and _structure_, so up migrations have to create backups if they are needed by down migrations. After v1, no existing migrations should ever be modified, but new migrations should be created.
- No PUT or PATCH or whatever shinanigans. Only use GET, POST and DELETE.

TODO(90): Finish this draft then clean it up so that it looks like proper organized markdown, then do the do-it's within it.