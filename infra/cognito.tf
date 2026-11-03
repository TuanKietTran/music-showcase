resource "aws_cognito_user_pool" "main" {
  name                     = "${local.name}-dev"
  username_attributes      = ["email"]
  auto_verified_attributes = ["email"]
  deletion_protection      = "INACTIVE" # dev stack: aws-destroy.sh must be able to remove it
  password_policy {
    minimum_length    = 12
    require_lowercase = true
    require_uppercase = true
    require_numbers   = true
    require_symbols   = false
  }
  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }
  admin_create_user_config { allow_admin_create_user_only = true } # no self sign-up: an admin creates accounts and sets role/status/quota
  # Attributes the server maps to subject attributes (docs/aws.md). Mutable, but clients can NOT write them (see the app client).
  schema {
    name                = "role"
    attribute_data_type = "String"
    mutable             = true
    string_attribute_constraints {
      min_length = 1
      max_length = 16
    }
  }
  schema {
    name                = "status"
    attribute_data_type = "String"
    mutable             = true
    string_attribute_constraints {
      min_length = 1
      max_length = 16
    }
  }
  schema {
    name                = "quota_bytes"
    attribute_data_type = "Number"
    mutable             = true
    number_attribute_constraints {
      min_value = "0"
    }
  }
}

# Groups mirror the roles (precedence: admin wins). The server uses custom:role first and falls back to the highest group.
resource "aws_cognito_user_group" "roles" {
  for_each     = { admin = 1, uploader = 2, listener = 3 }
  name         = each.key
  user_pool_id = aws_cognito_user_pool.main.id
  precedence   = each.value
  description  = "Music Showcase ${each.key}"
}

resource "aws_cognito_user_pool_domain" "main" {
  domain       = "${local.name}-${random_string.pool_domain.result}"
  user_pool_id = aws_cognito_user_pool.main.id
}

# Public client: no secret, authorization-code flow with PKCE (hosted UI). The server expects the ID token as the Bearer token.
resource "aws_cognito_user_pool_client" "web" {
  name                                 = "${local.name}-web"
  user_pool_id                         = aws_cognito_user_pool.main.id
  generate_secret                      = false
  allowed_oauth_flows_user_pool_client = true
  allowed_oauth_flows                  = ["code"]
  allowed_oauth_scopes                 = ["openid", "email", "profile"]
  supported_identity_providers         = ["COGNITO"]
  callback_urls                        = ["${local.app_origin}/", "http://localhost:5173/"]
  logout_urls                          = ["${local.app_origin}/", "http://localhost:5173/"]
  explicit_auth_flows                  = concat(["ALLOW_USER_SRP_AUTH", "ALLOW_REFRESH_TOKEN_AUTH"], var.enable_admin_auth_flow ? ["ALLOW_ADMIN_USER_PASSWORD_AUTH"] : [])
  prevent_user_existence_errors        = "ENABLED"
  enable_token_revocation              = true
  access_token_validity                = 15 # minutes: bounds how long a revoked/changed account keeps its old claims
  id_token_validity                    = 15
  refresh_token_validity               = 30
  token_validity_units {
    access_token  = "minutes"
    id_token      = "minutes"
    refresh_token = "days"
  }
  # Users must not be able to promote themselves: custom:role/status/quota_bytes are NOT writable by the client.
  read_attributes  = ["email", "email_verified", "custom:role", "custom:status", "custom:quota_bytes"]
  write_attributes = ["email"]
}
