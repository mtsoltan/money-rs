Front end should allow:

# Entries functionality
- Listing of entries
- Filtering of entries based on source (including secondary source) / category / currency / entry type
- Filtering of entries based on amount (gte / lte / eq)
- Filtering of entries based on date (gte / lte / eq) (can quick select a month or a year)
- Search of entries based on description
- Multi-selecting entries, with select all that selects all entries in the search / filter.
- Sort based on any field (sorts on amount and date are possible when fetching data to the table, sorts on other columns are post-fetch front-end based)
- Displays sum of selected entries (all entries if none selected)
- Displays average per month of selected entries
- Displays sum per category per month of selected entries
- Bulk editing of selected entries (can change category / description / currency / source / secondary source / entry type / date)
- Recreation of individual entries (allows changing the above, and conversion rate, date and amount - by deleting the entry and creating a new one - has confirmation)
- Archival / deletion of entries
- Creation of new entries

# Categories functionality
- Monthly sum of entries for this category, as well as averages
- For now the above displays for 1 year, in 1 month intervals, but will be changed to be more flexible in future versions
- Create-update-read on categories, and archival of them
- See which entries contributed the most to a category within a timeframe, defaulting to the past month (uses /entry api)
- TODO(80): DESIGN: decide the rest of categories functionality

# Currencies functionality
- Change display currency (for all of the above) - defaults to the fixed currency of the user
- TODO(80): DESIGN: decide the rest of currencies functionality

# Sources functionality
- TODO(80): DESIGN: decide the rest of sources functionality
- FE Currency should display: The balance exists in the following sources: <_>

# General front-end
- Tables, with sorts and filters, and a toggle that makes these sorts / filters client-side vs. re-fetch using /entry api
- Printing
- Time selection for filtering looks like that of datadog / aws, lowest granularity in the range selection is a day

# To-do's

TODO(70): EXTRA: Automatic price fetching from an online API
- The currencies list page should show both the DB-specified rate, and the internet-fetched rate, and have three options to update rates
- Internet fetched rates should support all fiat, well-known crypto, and gold. These will use three different APIs that still need to be decided.
- The first is "Update Rate" beside each individual currency, which will skip the update modal and update the rate using one from the internet (request to that endpoint that takes many currencies in POST body, but specifying only that one currency)
- The second is the "Update All Rates" at the top of the page, which should do this for all currencies (single request)
- The third is autoupdate, which updates the rate on all currencies daily for that user.
  - This would mean that when a user creates a currency, if the name and the name of their fixed matches a new "global_currency_pair", it will be linked. A cron job will update all global currency pairs which are matched to at least one user every day, and then update all the currencies matched to them for those users.
- These do not replace manual rate update capability from the update button.
- Historical rates should be saved in a table.

TODO(70): EXTRA: Automatic tagging of entries:
  - allow a box for amount + currency (prefix / suffix) and a dropdown for currency - locked if typed inside the box
  - box placeholder should have currency as prefix
  - Third input is for description, with an AI button beside it, that when tapped will try to fill all the remaining inputs from AI
  - This input should have autocomplete from existing ones (combo box like)
  Automatically tag:
  - entry type - deduce from description
  - category - deduce from description
  - source id - deduce from description
  - secondary source id - deduce from description
  - date if specified in description, otherwise current date
  - description (updated to no longer have category, date, and entry type),
  Deduction from description works by trying to match to an existing description in database (by strict matching, or asking an LLM),
  and if not, by asking an LLM to come up with something of its own


todos: 8 todos: one 40, two 60s, 2 70s are left for the backend, then three 80s between the backend and the front-end.
I can start work on front-end, and then v1 of this

The seven todos in contributing.md I can ignore for now until I have finished the front-end.