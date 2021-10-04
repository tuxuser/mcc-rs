use std::str::FromStr;

use crate::{schemas, Result};
#[cfg(test)]
use mockito;
use reqwest::header::ACCEPT_LANGUAGE;

#[derive(Debug, Clone)]
pub enum RecipeType {
    Default,
    Live,
    Beta,
}

impl ToString for RecipeType {
    fn to_string(&self) -> String {
        match self {
            RecipeType::Default => "default".into(),
            RecipeType::Live => "live".into(),
            RecipeType::Beta => "beta".into(),
        }
    }
}

impl FromStr for RecipeType {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(RecipeType::Default),
            "live" => Ok(RecipeType::Live),
            "beta" => Ok(RecipeType::Beta),
            _ => {
                let e = format!("Invalid string for RecipeType provided: {}", s);
                Err(e.into())
            }
        }
    }
}

/// Api provides a client for fetching APK updates and recipes
pub struct Api {
    session: reqwest::Client,
    default_language: String,
}

impl Api {
    /// Create new instance of Api
    ///
    /// Language is provided in ISO 639-1 format
    //  (e.g. "de", "it", "fr", "pl", "en", "es")
    pub fn new(language: &str) -> Self {
        Self {
            session: reqwest::Client::new(),
            default_language: language.to_string(),
        }
    }
}

/// Helpers
impl Api {
    fn create_url(path: &str) -> Result<reqwest::Url> {
        #[cfg(not(test))]
        let base = reqwest::Url::parse("https://mc20.monsieur-cuisine.com")?;

        #[cfg(test)]
        let base = reqwest::Url::parse(&mockito::server_url())?;

        Ok(base.join(path)?)
    }
}

/// APK / Android endpoints
impl Api {
    const DOWNLOAD_PATH: &'static str = "/666a60bc-0ce2-4878-9e3b-23ba3ceaba5a";

    /// Get a list of MC2 APK files available to download
    pub async fn get_apk_updates(&self) -> Result<Vec<String>> {
        let url = Api::create_url(&format!("{}/{}", Api::DOWNLOAD_PATH, "versions.txt"))?;

        let result = self.session.get(url).send().await?.text().await?;

        let apk_urls = result
            .trim_end()
            .split('\n')
            .map(|x| {
                let url = Api::create_url(&format!("{}/{}", Api::DOWNLOAD_PATH, x))
                    .expect("Failed to create URL");

                url.to_string()
            })
            .collect::<Vec<String>>();

        Ok(apk_urls)
    }
}

// Recipes
impl Api {
    /// Helper function to wrap calls against Recipe endpoint
    async fn get_recipe_endpoint(
        &self,
        endpoint: &str,
        language: Option<&str>,
        recipe_type: Option<RecipeType>,
    ) -> Result<reqwest::Response> {
        let recipe_type = recipe_type
            .or(Some(RecipeType::Default))
            .unwrap()
            .to_string();
        let language = language.or(Some(&self.default_language)).unwrap();

        let url = Api::create_url(&format!("/mcc/api/v1/recipe/{}", endpoint))?;
        let result = self
            .session
            .get(url)
            .header(ACCEPT_LANGUAGE, language)
            .header("X-Recipe-Type", recipe_type)
            .send()
            .await?;

        Ok(result)
    }

    /// Get recipe ids for particular language / recipe type
    pub async fn get_recipe_ids(
        &self,
        language: Option<&str>,
        recipe_type: Option<RecipeType>,
    ) -> Result<Vec<u32>> {
        let result = self
            .get_recipe_endpoint("ids", language, recipe_type)
            .await?
            .json::<schemas::RecipeIds>()
            .await?;

        Ok(result.ids)
    }

    /// Get single recipe by id for particular language / recipe type
    pub async fn get_recipe(
        &self,
        id: u32,
        language: Option<&str>,
        recipe_type: Option<RecipeType>,
    ) -> Result<schemas::Recipe> {
        let result = self
            .get_recipe_endpoint(&id.to_string(), language, recipe_type)
            .await?
            .json::<schemas::Recipe>()
            .await?;

        Ok(result)
    }

    /// Get all recipes for particular language / recipe type
    pub async fn get_recipes(
        &self,
        language: Option<&str>,
        recipe_type: Option<RecipeType>,
    ) -> Result<Vec<schemas::Recipe>> {
        let result = self
            .get_recipe_endpoint("all", language, recipe_type)
            .await?
            .json::<Vec<schemas::Recipe>>()
            .await?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::get_testdata;
    use mockito::mock;
    use rstest::*;

    const DEFAULT_LANGUAGE: &str = "de";

    #[fixture]
    fn client() -> Api {
        Api {
            session: reqwest::Client::new(),
            default_language: DEFAULT_LANGUAGE.into(),
        }
    }

    #[rstest]
    #[tokio::test]
    async fn get_apk_updates(client: Api) {
        let body = get_testdata("versions.txt").expect("Failed to get testdata");

        let _m = mock("GET", "/666a60bc-0ce2-4878-9e3b-23ba3ceaba5a/versions.txt")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();

        let res = client
            .get_apk_updates()
            .await
            .expect("Failed to fetch apk updates");

        assert!(res.len() == 3);
    }

    #[rstest]
    #[case::de_beta(Some("de"), Some(RecipeType::Beta))]
    #[case::de_none(Some("de"), None)]
    #[case::en_default(Some("en"), Some(RecipeType::Default))]
    #[case::en_none(Some("en"), None)]
    #[case::fr_live(Some("fr"), Some(RecipeType::Live))]
    #[case::fr_none(Some("fr"), None)]
    #[case::pl_default(Some("pl"), Some(RecipeType::Default))]
    #[case::pl_none(Some("pl"), None)]
    #[case::es_beta(Some("es"), Some(RecipeType::Beta))]
    #[case::none_live(None, Some(RecipeType::Live))]
    #[case::none_none(None, None)]
    #[tokio::test]
    async fn get_recipe_ids(
        client: Api,
        #[case] language: Option<&str>,
        #[case] recipe_type: Option<RecipeType>,
    ) {
        let body = get_testdata("recipe_ids.json").expect("Failed to get testdata");

        let recipe_type_str = recipe_type
            .as_ref()
            .or(Some(&RecipeType::Default))
            .unwrap()
            .to_string();
        let language_str = language.or(Some(DEFAULT_LANGUAGE)).unwrap().to_string();

        let _m = mock("GET", "/mcc/api/v1/recipe/ids")
            .match_header(&ACCEPT_LANGUAGE.to_string(), language_str.as_str())
            .match_header("X-Recipe-Type", recipe_type_str.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();

        let res = client
            .get_recipe_ids(language, recipe_type)
            .await
            .expect("Failed to get recipe ids");

        assert_eq!(res.len(), 2);
    }

    #[rstest]
    #[case::de_beta(Some("de"), Some(RecipeType::Beta))]
    #[case::de_none(Some("de"), None)]
    #[case::en_default(Some("en"), Some(RecipeType::Default))]
    #[case::en_none(Some("en"), None)]
    #[case::fr_live(Some("fr"), Some(RecipeType::Live))]
    #[case::fr_none(Some("fr"), None)]
    #[case::pl_default(Some("pl"), Some(RecipeType::Default))]
    #[case::pl_none(Some("pl"), None)]
    #[case::es_beta(Some("es"), Some(RecipeType::Beta))]
    #[case::none_live(None, Some(RecipeType::Live))]
    #[case::none_none(None, None)]
    #[tokio::test]
    async fn get_recipe(
        client: Api,
        #[case] language: Option<&str>,
        #[case] recipe_type: Option<RecipeType>,
    ) {
        let body = get_testdata("recipe_single_25011.json").expect("Failed to get testdata");

        let recipe_type_str = recipe_type
            .as_ref()
            .or(Some(&RecipeType::Default))
            .unwrap()
            .to_string();
        let language_str = language.or(Some(DEFAULT_LANGUAGE)).unwrap().to_string();

        let _m = mock("GET", "/mcc/api/v1/recipe/25011")
            .match_header(&ACCEPT_LANGUAGE.to_string(), language_str.as_str())
            .match_header("X-Recipe-Type", recipe_type_str.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();

        let res = client
            .get_recipe(25011, language, recipe_type)
            .await
            .expect("Failed to get recipe");

        assert_eq!(res.data.id, 25011);
    }

    #[rstest]
    #[case::de_beta(Some("de"), Some(RecipeType::Beta))]
    #[case::de_none(Some("de"), None)]
    #[case::en_default(Some("en"), Some(RecipeType::Default))]
    #[case::en_none(Some("en"), None)]
    #[case::fr_live(Some("fr"), Some(RecipeType::Live))]
    #[case::fr_none(Some("fr"), None)]
    #[case::pl_default(Some("pl"), Some(RecipeType::Default))]
    #[case::pl_none(Some("pl"), None)]
    #[case::es_beta(Some("es"), Some(RecipeType::Beta))]
    #[case::none_live(None, Some(RecipeType::Live))]
    #[case::none_none(None, None)]
    #[tokio::test]
    async fn get_recipes_all(
        client: Api,
        #[case] language: Option<&str>,
        #[case] recipe_type: Option<RecipeType>,
    ) {
        let recipe_type_str = recipe_type
            .as_ref()
            .or(Some(&RecipeType::Default))
            .unwrap()
            .to_string();
        let language_str = language.or(Some(DEFAULT_LANGUAGE)).unwrap().to_string();

        let filename = format!("recipe_all_{}.json", recipe_type_str);
        let body = get_testdata(&filename).expect("Failed to get testdata");

        let _m = mock("GET", "/mcc/api/v1/recipe/all")
            .match_header(&ACCEPT_LANGUAGE.to_string(), language_str.as_str())
            .match_header("X-Recipe-Type", recipe_type_str.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();

        let res = client
            .get_recipes(language, recipe_type)
            .await
            .expect("Failed to get recipe");

        assert!(!res.is_empty());
    }
}
