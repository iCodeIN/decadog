#![deny(clippy::all)]

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::Hasher;

use chrono::{DateTime, FixedOffset};

mod core;
pub mod error;
pub mod github;
pub mod secret;
pub mod zenhub;

pub use crate::core::{AssignedTo, Sprint};
pub use error::Error;
use github::{
    paginate::PaginatedSearch, Direction, Issue, IssueUpdate, Milestone, MilestoneUpdate,
    OrganisationMember, Repository, SearchIssues, SearchQueryBuilder, State,
};
use zenhub::{Board, Pipeline, PipelinePosition, StartDate, Workspace};

/// Decadog client, used to abstract complex tasks over several APIs.
pub struct Client<'a> {
    owner: &'a str,
    repo: &'a str,
    github: &'a github::Client,
    zenhub: &'a zenhub::Client,

    id: u64,
}

impl<'a> fmt::Debug for Client<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Decadog client {}", self.id)
    }
}

impl<'a> Client<'a> {
    /// Create a new client that can make requests to the Github API using token auth.
    pub fn new(
        owner: &'a str,
        repo: &'a str,
        github: &'a github::Client,
        zenhub: &'a zenhub::Client,
    ) -> Result<Client<'a>, Error> {
        let mut hasher = DefaultHasher::new();
        hasher.write(owner.as_bytes());
        hasher.write(repo.as_bytes());
        hasher.write(&github.id().to_be_bytes());
        hasher.write(&zenhub.id().to_be_bytes());
        let id = hasher.finish();

        Ok(Client {
            id,
            owner,
            repo,
            github,
            zenhub,
        })
    }

    pub fn owner(&self) -> &str {
        self.owner
    }

    pub fn repo(&self) -> &str {
        self.repo
    }

    /// Get Zenhub StartDate for a Github Milestone.
    pub fn get_start_date(
        &self,
        repository: &Repository,
        milestone: &Milestone,
    ) -> Result<StartDate, Error> {
        self.zenhub.get_start_date(repository.id, milestone.number)
    }

    /// Get Zenhub first workspace for a repository.
    pub fn get_first_workspace(&self, repository: &Repository) -> Result<Workspace, Error> {
        self.zenhub.get_first_workspace(repository.id)
    }

    /// Get Zenhub board for a repository.
    pub fn get_board(
        &self,
        repository: &Repository,
        workspace: &Workspace,
    ) -> Result<Board, Error> {
        self.zenhub.get_board(repository.id, &workspace.id)
    }

    /// Get Zenhub issue metadata.
    pub fn get_zenhub_issue(
        &self,
        repository: &Repository,
        issue: &Issue,
    ) -> Result<zenhub::Issue, Error> {
        self.zenhub.get_issue(repository.id, issue.number)
    }

    /// Set Zenhub issue estimate.
    pub fn set_estimate(
        &self,
        repository: &Repository,
        issue: &Issue,
        estimate: u32,
    ) -> Result<(), Error> {
        self.zenhub
            .set_estimate(repository.id, issue.number, estimate)
    }

    /// Get sprint for milestone.
    pub fn get_sprint(
        &self,
        repository: &Repository,
        milestone: Milestone,
    ) -> Result<Sprint, Error> {
        let start_date = self.get_start_date(repository, &milestone)?;
        Ok(Sprint {
            milestone,
            start_date,
        })
    }

    /// Create a new sprint.
    pub fn create_sprint(
        &self,
        repository: &Repository,
        sprint_number: &str,
        start_date: DateTime<FixedOffset>,
        due_on: DateTime<FixedOffset>,
    ) -> Result<Sprint, Error> {
        let mut milestone_spec = MilestoneUpdate::default();
        milestone_spec.title = Some(format!("Sprint {}", sprint_number));
        milestone_spec.due_on = Some(due_on);

        let milestone = self
            .github
            .create_milestone(self.owner, self.repo, &milestone_spec)?;

        let start_date = start_date.into();
        let start_date =
            self.zenhub
                .set_start_date(repository.id, milestone.number, &start_date)?;
        Ok(Sprint {
            milestone,
            start_date,
        })
    }

    /// Move issue to a Zenhub pipeline.
    pub fn move_issue_to_pipeline(
        &self,
        repository: &Repository,
        workspace: &Workspace,
        issue: &Issue,
        pipeline: &Pipeline,
    ) -> Result<(), Error> {
        let mut position = PipelinePosition::default();
        position.pipeline_id = pipeline.id.clone();

        self.zenhub
            .move_issue(repository.id, &workspace.id, issue.number, &position)
    }

    /// Get a repository from the API.
    pub fn get_repository(&self) -> Result<Repository, Error> {
        self.github.get_repository(self.owner, self.repo)
    }

    /// Get an issue from the API.
    pub fn get_issue(&self, issue_number: u32) -> Result<Issue, Error> {
        self.github.get_issue(self.owner, self.repo, issue_number)
    }

    /// Get milestones from the API.
    pub fn get_milestones(&self) -> Result<Vec<Milestone>, Error> {
        self.github.get_milestones(self.owner, self.repo)
    }

    /// Assign an issue to a milestone. Passing `None` will set to no milestone.
    ///
    /// This will overwrite an existing milestone, if present.
    pub fn assign_issue_to_milestone(
        &self,
        issue: &Issue,
        milestone: Option<&Milestone>,
    ) -> Result<Issue, Error> {
        let mut update = IssueUpdate::default();
        update.milestone = Some(milestone.map(|milestone| milestone.number));

        self.github
            .patch_issue(&self.owner, &self.repo, issue.number, &update)
    }

    /// Assign an organisation member to an issue.
    ///
    /// This will overwrite any existing assignees, if present.
    pub fn assign_member_to_issue(
        &self,
        member: &OrganisationMember,
        issue: &Issue,
    ) -> Result<Issue, Error> {
        let mut update = IssueUpdate::default();
        update.assignees = Some(vec![member.login.clone()]);

        self.github
            .patch_issue(&self.owner, &self.repo, issue.number, &update)
    }

    /// Get issues by the given query, in ascending order of time updated.
    pub fn search_issues(
        &self,
        query_builder: &mut SearchQueryBuilder,
    ) -> Result<PaginatedSearch<Issue>, Error> {
        let query = SearchIssues {
            q: query_builder
                .owner_repo(self.owner, self.repo)
                .issue()
                .build(),
            sort: Some("updated"),
            order: Some(Direction::Ascending),
            per_page: Some(100),
        };
        self.github.search_issues(&query)
    }

    /// Get organisation members.
    pub fn get_members(&self) -> Result<Vec<OrganisationMember>, Error> {
        self.github.get_members(self.owner)
    }

    /// Update milestone title with provided title
    pub fn update_milestone_title(
        &self,
        milestone: &Milestone,
        new_title: String,
    ) -> Result<Milestone, Error> {
        let mut update = MilestoneUpdate::default();
        update.title = Some(new_title);
        self.github
            .patch_milestone(&self.owner, &self.repo, milestone.number, &update)
    }

    /// Close milestone.
    pub fn close_milestone(&self, milestone: &Milestone) -> Result<Milestone, Error> {
        let mut update = MilestoneUpdate::default();
        update.state = Some(State::Closed);
        self.github
            .patch_milestone(&self.owner, &self.repo, milestone.number, &update)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, NaiveDate, TimeZone};
    use lazy_static::lazy_static;
    use mockito::mock;
    use pretty_assertions::assert_eq;

    use super::github::{tests::MOCK_GITHUB_CLIENT, State};
    use super::zenhub::tests::MOCK_ZENHUB_CLIENT;
    use super::*;

    const OWNER: &str = "tommilligan";
    const REPO: &str = "decadog";
    lazy_static! {
        pub static ref MOCK_CLIENT: Client<'static> =
            Client::new(OWNER, REPO, &MOCK_GITHUB_CLIENT, &MOCK_ZENHUB_CLIENT)
                .expect("Couldn't create mock client");
    }

    #[test]
    fn test_get_issues_closed_after() {
        let body = r#"{
  "incomplete_results": false,
  "items": []
}"#;
        let mock = mock("GET", "/search/issues?q=state%3Aclosed+closed%3A%3E%3D2011-04-22+repo%3Atommilligan%2Fdecadog+type%3Aissue&sort=updated&order=asc&per_page=100")
            .match_header("authorization", "token mock_token")
            .with_status(200)
            .with_body(body)
            .create();

        let issues = MOCK_CLIENT
            .search_issues(
                &mut SearchQueryBuilder::new().closed_on_or_after(
                    &FixedOffset::east(0)
                        .from_utc_datetime(&NaiveDate::from_ymd(2011, 4, 22).and_hms(13, 33, 48)),
                ),
            )
            .unwrap()
            .collect::<Result<Vec<Issue>, _>>()
            .unwrap();

        mock.assert();

        assert_eq!(issues, vec![]);
    }

    #[test]
    fn test_get_milestone_open_issues() {
        let body = r#"{
  "incomplete_results": false,
  "items": []
}"#;
        let mock = mock("GET", "/search/issues?q=state%3Aopen+milestone%3A%22Sprint+2%22+repo%3Atommilligan%2Fdecadog+type%3Aissue&sort=updated&order=asc&per_page=100")
            .match_header("authorization", "token mock_token")
            .with_status(200)
            .with_body(body)
            .create();

        let issues = MOCK_CLIENT
            .search_issues(
                SearchQueryBuilder::new()
                    .state(&State::Open)
                    .milestone("Sprint 2"),
            )
            .unwrap()
            .collect::<Result<Vec<Issue>, _>>()
            .unwrap();

        mock.assert();

        assert_eq!(issues, vec![]);
    }
}
