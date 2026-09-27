resource "aws_s3_bucket" "site" {
  bucket = "uze-site"
  tags = {
    Environment = "production"
  }
}
