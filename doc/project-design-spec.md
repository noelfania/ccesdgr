# 📋 프로젝트 설계서

> **문서 버전**: v1.0.0
> **최초 작성일**: YYYY-MM-DD
> **최종 수정일**: YYYY-MM-DD
> **작성자**: 홍길동
> **검토자**: -
> **상태**: `초안` / `검토중` / `확정` / `deprecated`

---

## 목차

1. [개요](#1-개요)
2. [시스템 디자인](#2-시스템-디자인)
3. [화면 기본 설계](#3-화면-기본-설계)
4. [화면 상세 설계](#4-화면-상세-설계)
5. [백엔드 기본 설계](#5-백엔드-기본-설계)
6. [백엔드 상세 설계](#6-백엔드-상세-설계)
7. [UI/UX 공통 사양](#7-uiux-공통-사양)
8. [빌드 및 배포](#8-빌드-및-배포)
9. [부록](#9-부록)

---

## 1. 개요

### 1.1 프로젝트 소개

| 항목 | 내용 |
|---|---|
| 프로젝트명 | - |
| 서비스명 | - |
| 목적 | - |
| 대상 사용자 | - |
| 플랫폼 | Web / Mobile Web / iOS / Android |

### 1.2 배경 및 목표

> 이 서비스를 왜 만드는가, 어떤 문제를 해결하는가를 서술합니다.

**배경**

- ...

**목표**

- [ ] 목표 1
- [ ] 목표 2
- [ ] 목표 3

### 1.3 범위 (Scope)

**In-Scope (포함)**

- ...

**Out-of-Scope (제외)**

- ...

### 1.4 일정

| 단계 | 기간 | 담당 | 비고 |
|---|---|---|---|
| 요구사항 분석 | YYYY-MM-DD ~ YYYY-MM-DD | - | |
| 설계 | YYYY-MM-DD ~ YYYY-MM-DD | - | |
| 개발 | YYYY-MM-DD ~ YYYY-MM-DD | - | |
| QA | YYYY-MM-DD ~ YYYY-MM-DD | - | |
| 배포 | YYYY-MM-DD | - | |

### 1.5 용어 정의

| 용어 | 설명 |
|---|---|
| 용어1 | 설명 |
| 용어2 | 설명 |

---

## 2. 시스템 디자인

### 2.1 기술 스택

| 영역 | 기술 | 버전 | 비고 |
|---|---|---|---|
| Frontend | React | 18.x | |
| Styling | Tailwind CSS | 3.x | |
| State | Zustand | - | |
| Backend | Node.js / NestJS | 20.x | |
| Database | PostgreSQL | 15.x | |
| Cache | Redis | 7.x | |
| Storage | AWS S3 | - | |
| CDN | CloudFront | - | |
| 인프라 | AWS ECS | - | |

### 2.2 시스템 아키텍처

```
[Client (Browser)]
       │
       ▼
[CDN / CloudFront]
       │
       ▼
[API Gateway]
       │
  ┌────┴────┐
  ▼         ▼
[API Server] [Static Assets]
  │
  ├──▶ [PostgreSQL DB]
  ├──▶ [Redis Cache]
  └──▶ [AWS S3]
```

> 실제 프로젝트에 맞게 다이어그램을 교체하세요. (Mermaid / draw.io 권장)

```mermaid
graph TD
    A[Client] --> B[API Gateway]
    B --> C[Auth Service]
    B --> D[Main API Server]
    D --> E[(PostgreSQL)]
    D --> F[(Redis)]
    D --> G[S3]
```

### 2.3 환경 구성

| 환경 | 용도 | URL | 비고 |
|---|---|---|---|
| local | 개발자 로컬 | localhost:3000 | |
| dev | 개발 통합 | dev.example.com | |
| staging | QA / 검증 | staging.example.com | |
| production | 운영 | www.example.com | |

### 2.4 외부 연동 시스템

| 시스템명 | 용도 | 연동 방식 | 담당 |
|---|---|---|---|
| Firebase Auth | 소셜 로그인 | REST API | - |
| 채널톡 | 고객 지원 채팅 | Script 삽입 | - |
| Google Analytics | 사용자 분석 | SDK | - |

### 2.5 보안 설계

- **인증**: JWT (Access Token 15분 / Refresh Token 7일)
- **인가**: RBAC (Role-Based Access Control)
- **통신**: HTTPS 강제
- **입력값 검증**: 서버사이드 유효성 검사 필수
- **민감정보**: 환경변수(.env) 관리, 소스코드 미포함
- **CORS**: 허용 도메인 화이트리스트 설정

---

## 3. 화면 기본 설계

### 3.1 사이트맵

```
Root (/)
├── 인증
│   ├── 로그인 (/login)
│   └── 회원가입 (/signup)
├── 대시보드 (/dashboard)
├── 게시판
│   ├── 목록 (/board)
│   ├── 상세 (/board/:id)
│   └── 작성 (/board/write)
└── 마이페이지 (/mypage)
    ├── 프로필 (/mypage/profile)
    └── 설정 (/mypage/settings)
```

### 3.2 화면 목록

| 화면 ID | 화면명 | URL | 인증 필요 | 비고 |
|---|---|---|---|---|
| SCR-001 | 로그인 | /login | ❌ | |
| SCR-002 | 회원가입 | /signup | ❌ | |
| SCR-003 | 대시보드 | /dashboard | ✅ | |
| SCR-004 | 게시판 목록 | /board | ✅ | |
| SCR-005 | 게시글 상세 | /board/:id | ✅ | |
| SCR-006 | 게시글 작성 | /board/write | ✅ | |
| SCR-007 | 마이페이지 | /mypage | ✅ | |

### 3.3 공통 레이아웃

**레이아웃 종류**

| 레이아웃 ID | 이름 | 적용 화면 | 구성 |
|---|---|---|---|
| LYT-001 | GNB 레이아웃 | 로그인 후 전 화면 | Header + Sidebar + Content |
| LYT-002 | 풀스크린 | 로그인, 회원가입 | Content Only |
| LYT-003 | 모달 오버레이 | 각종 팝업 | 기존 화면 위에 오버레이 |

**GNB (Global Navigation Bar) 구성**

- 로고
- 메인 메뉴
- 알림 아이콘
- 사용자 프로필 드롭다운 (로그아웃, 설정)

---

## 4. 화면 상세 설계

> 각 화면별로 아래 템플릿을 복사하여 작성합니다.

---

### SCR-001 · 로그인

**기본 정보**

| 항목 | 내용 |
|---|---|
| 화면명 | 로그인 |
| URL | /login |
| 레이아웃 | LYT-002 (풀스크린) |
| 인증 필요 | ❌ |
| 관련 API | POST /api/auth/login |

**화면 구성 요소**

| 컴포넌트 | 타입 | 필수 | 설명 |
|---|---|---|---|
| 이메일 | Input (email) | ✅ | |
| 비밀번호 | Input (password) | ✅ | |
| 로그인 버튼 | Button | ✅ | |
| 소셜 로그인 | Button Group | - | 구글, 카카오 |
| 회원가입 링크 | Link | - | /signup 이동 |

**유효성 검사**

| 필드 | 조건 | 에러 메시지 |
|---|---|---|
| 이메일 | 이메일 형식 | "올바른 이메일 주소를 입력해주세요." |
| 이메일 | 필수 입력 | "이메일을 입력해주세요." |
| 비밀번호 | 필수 입력 | "비밀번호를 입력해주세요." |
| 비밀번호 | 최소 8자 | "비밀번호는 8자 이상이어야 합니다." |

**동작 흐름**

```
1. 사용자가 이메일/비밀번호 입력
2. 로그인 버튼 클릭
3. 클라이언트 유효성 검사
   - 실패 → 인라인 에러 메시지 표시
   - 성공 → 4번으로
4. POST /api/auth/login 호출
   - 성공 → Access Token 저장 → /dashboard 리다이렉트
   - 실패 (401) → "이메일 또는 비밀번호가 올바르지 않습니다." 토스트 표시
   - 실패 (기타) → 공통 에러 토스트 표시
```

**특이사항 / 예외 처리**

- 로그인 상태에서 /login 접근 시 /dashboard 자동 리다이렉트
- 5회 연속 실패 시 30분 잠금 처리 (서버 응답 기준)
- 비밀번호 입력 중 Caps Lock 감지 시 안내 툴팁 표시

---

### SCR-004 · 게시판 목록

**기본 정보**

| 항목 | 내용 |
|---|---|
| 화면명 | 게시판 목록 |
| URL | /board |
| 레이아웃 | LYT-001 (GNB) |
| 인증 필요 | ✅ |
| 관련 API | GET /api/posts |

**화면 구성 요소**

| 컴포넌트 | 설명 |
|---|---|
| 검색창 | 제목/내용 키워드 검색 |
| 카테고리 필터 | 전체 / 공지 / 일반 |
| 게시글 목록 테이블 | 번호, 제목, 작성자, 날짜, 조회수 |
| 페이지네이션 | 페이지당 20개, 최대 10페이지 노출 |
| 글쓰기 버튼 | 권한자만 노출 |

**페이지네이션 정책**

- 기본 정렬: 최신순 (created_at DESC)
- 페이지당 항목 수: 20개
- URL 쿼리: `/board?page=1&size=20&keyword=검색어&category=all`

---

## 5. 백엔드 기본 설계

### 5.1 API 설계 원칙

- RESTful API 설계 준수
- 응답 형식 통일 (아래 공통 응답 포맷 참조)
- HTTP 상태 코드 의미에 맞게 사용
- API 버전 관리: URL prefix 방식 (`/api/v1/...`)

### 5.2 공통 응답 포맷

**성공 응답**

```json
{
  "success": true,
  "data": { ... },
  "message": "요청이 성공적으로 처리되었습니다."
}
```

**목록 응답 (페이지네이션)**

```json
{
  "success": true,
  "data": {
    "items": [ ... ],
    "total": 100,
    "page": 1,
    "size": 20,
    "totalPages": 5
  }
}
```

**실패 응답**

```json
{
  "success": false,
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "입력값이 올바르지 않습니다.",
    "details": [ ... ]
  }
}
```

### 5.3 에러 코드 정의

| HTTP Status | Error Code | 설명 |
|---|---|---|
| 400 | VALIDATION_ERROR | 입력값 유효성 오류 |
| 401 | UNAUTHORIZED | 인증 실패 / 토큰 없음 |
| 403 | FORBIDDEN | 권한 없음 |
| 404 | NOT_FOUND | 리소스 없음 |
| 409 | CONFLICT | 중복 데이터 |
| 429 | TOO_MANY_REQUESTS | Rate Limit 초과 |
| 500 | INTERNAL_SERVER_ERROR | 서버 내부 오류 |

### 5.4 인증 / 인가

- **인증 방식**: JWT Bearer Token
- **토큰 전달**: `Authorization: Bearer <access_token>` 헤더
- **Access Token 만료**: 15분
- **Refresh Token 만료**: 7일 (HttpOnly Cookie)
- **토큰 갱신**: `POST /api/auth/refresh`

### 5.5 데이터베이스 설계 (ERD 요약)

```
users
├── id (PK, UUID)
├── email (UNIQUE)
├── password_hash
├── name
├── role (enum: admin, user)
├── created_at
└── updated_at

posts
├── id (PK, UUID)
├── user_id (FK → users.id)
├── category_id (FK → categories.id)
├── title
├── content
├── view_count
├── created_at
└── updated_at

categories
├── id (PK)
├── name
└── sort_order
```

> 전체 ERD는 별첨 문서 참조 또는 DB 툴(DBeaver, DataGrip) 스크린샷 첨부

---

## 6. 백엔드 상세 설계

> 각 API별로 아래 템플릿을 복사하여 작성합니다.

---

### API-001 · 로그인

| 항목 | 내용 |
|---|---|
| Method | POST |
| URL | /api/v1/auth/login |
| 인증 필요 | ❌ |
| 설명 | 이메일/비밀번호로 로그인 |

**Request**

```json
// Header
Content-Type: application/json

// Body
{
  "email": "user@example.com",
  "password": "password123!"
}
```

**Request 유효성 검사**

| 필드 | 타입 | 필수 | 조건 |
|---|---|---|---|
| email | string | ✅ | 이메일 형식 |
| password | string | ✅ | 8자 이상 |

**Response (200 OK)**

```json
{
  "success": true,
  "data": {
    "accessToken": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "user": {
      "id": "uuid",
      "email": "user@example.com",
      "name": "홍길동",
      "role": "user"
    }
  }
}
```

**Response (에러)**

| 상황 | Status | Error Code | 메시지 |
|---|---|---|---|
| 이메일 미존재 | 401 | UNAUTHORIZED | "이메일 또는 비밀번호가 올바르지 않습니다." |
| 비밀번호 불일치 | 401 | UNAUTHORIZED | "이메일 또는 비밀번호가 올바르지 않습니다." |
| 계정 잠금 | 423 | ACCOUNT_LOCKED | "계정이 잠겼습니다. 30분 후 다시 시도해주세요." |
| 유효성 실패 | 400 | VALIDATION_ERROR | 필드별 메시지 |

**처리 로직**

```
1. 요청 Body 유효성 검사
2. email로 users 테이블 조회
   - 미존재 → 401 반환 (보안상 구체적 이유 미노출)
3. 비밀번호 해시 비교 (bcrypt.compare)
   - 불일치 → 로그인 실패 카운트 +1 → 5회 초과 시 계정 잠금
   - 불일치 → 401 반환
4. Access Token 생성 (JWT, 15분)
5. Refresh Token 생성 (7일) → HttpOnly Cookie 설정
6. 200 응답
```

---

### API-002 · 게시글 목록 조회

| 항목 | 내용 |
|---|---|
| Method | GET |
| URL | /api/v1/posts |
| 인증 필요 | ✅ |
| 설명 | 게시글 목록 페이지네이션 조회 |

**Query Parameters**

| 파라미터 | 타입 | 필수 | 기본값 | 설명 |
|---|---|---|---|---|
| page | number | ❌ | 1 | 페이지 번호 |
| size | number | ❌ | 20 | 페이지당 항목 수 (최대 100) |
| keyword | string | ❌ | - | 제목+내용 검색어 |
| category | string | ❌ | all | 카테고리 필터 |
| sort | string | ❌ | latest | latest / popular |

**Response (200 OK)**

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "uuid",
        "title": "게시글 제목",
        "author": { "id": "uuid", "name": "홍길동" },
        "category": "일반",
        "viewCount": 123,
        "createdAt": "2024-01-01T00:00:00.000Z"
      }
    ],
    "total": 100,
    "page": 1,
    "size": 20,
    "totalPages": 5
  }
}
```

---

## 7. UI/UX 공통 사양

> 모든 화면에 일관되게 적용되는 인터랙션 규칙을 정의합니다.
> 에이전트는 이 사양을 기반으로 구현 판단을 내립니다.

---

### 7.1 모달 (Modal)

| 사양 항목 | 규칙 |
|---|---|
| 외부 클릭 | 모달 외부 영역 클릭 시 모달 닫힘 |
| ESC 키 | ESC 키 입력 시 모달 닫힘 |
| 스크롤 잠금 | 모달 열릴 때 body 스크롤 잠금 (`overflow: hidden`) |
| 중첩 모달 | 모달 위 모달 허용 (z-index 계층 관리) |
| 닫기 버튼 | 우측 상단 X 버튼 항상 노출 |
| 애니메이션 | fade-in + scale-up (150ms ease-out) |
| 배경 오버레이 | `rgba(0, 0, 0, 0.5)` |
| 중앙 정렬 | 수직/수평 중앙 정렬 (flexbox) |
| 최대 너비 | 기본 480px, 대형 모달 720px |
| 확인/취소 필수 모달 | 외부 클릭 비활성화, ESC 비활성화 |

---

### 7.2 토스트 알림 (Toast)

| 사양 항목 | 규칙 |
|---|---|
| 노출 위치 | 화면 우측 상단 (top: 20px, right: 20px) |
| 자동 닫힘 | 기본 3초 후 자동 닫힘 |
| 수동 닫힘 | X 버튼으로 즉시 닫기 가능 |
| 동시 노출 | 최대 3개까지 스택 노출 (초과 시 오래된 것부터 제거) |
| 종류 | success (초록) / error (빨강) / warning (주황) / info (파랑) |
| 애니메이션 | 오른쪽에서 슬라이드인 (200ms) |

---

### 7.3 로딩 상태 (Loading)

| 상황 | 처리 방식 |
|---|---|
| API 호출 중 버튼 | 버튼 비활성화 + 스피너 아이콘 표시 |
| 페이지 전체 로딩 | 전체 화면 오버레이 스피너 |
| 목록 데이터 로딩 | Skeleton UI 표시 |
| 이미지 로딩 | 블러 플레이스홀더 → 이미지 페이드인 |
| 최소 로딩 시간 | 200ms 미만 응답 시 로딩 UI 미표시 (깜빡임 방지) |

---

### 7.4 폼 유효성 검사 (Form Validation)

| 사양 항목 | 규칙 |
|---|---|
| 검사 시점 | 제출(submit) 시 전체 검사 / blur 시 해당 필드 검사 |
| 에러 표시 위치 | 입력 필드 바로 아래 인라인 에러 메시지 |
| 에러 스타일 | 입력 필드 border 빨강 + 에러 텍스트 빨강 |
| 포커스 이동 | 제출 시 첫 번째 에러 필드로 자동 포커스 |
| 서버 에러 | API 에러 메시지를 해당 필드 또는 폼 상단에 표시 |

---

### 7.5 빈 상태 (Empty State)

| 상황 | 처리 방식 |
|---|---|
| 목록 데이터 없음 | 일러스트 + 안내 문구 + CTA 버튼 표시 |
| 검색 결과 없음 | "'{keyword}'에 대한 검색 결과가 없습니다." 문구 표시 |
| 에러로 인한 데이터 없음 | 에러 안내 + "다시 시도" 버튼 표시 |

---

### 7.6 권한 제어

| 상황 | 처리 방식 |
|---|---|
| 비로그인 → 인증 필요 페이지 접근 | /login 리다이렉트 (returnUrl 파라미터 포함) |
| 로그인 → 로그인 페이지 접근 | /dashboard 리다이렉트 |
| 권한 없는 기능 버튼 | 버튼 미노출 또는 비활성화 (케이스별 정의) |
| 직접 URL 접근 권한 없음 | 403 에러 페이지 표시 |

---

### 7.7 반응형 브레이크포인트

| 이름 | 범위 | 레이아웃 |
|---|---|---|
| mobile | ~ 767px | 단일 컬럼, 사이드바 숨김 |
| tablet | 768px ~ 1023px | 사이드바 축소 아이콘만 |
| desktop | 1024px ~ | 풀 레이아웃 |

---

### 7.8 키보드 / 접근성

- 모든 인터랙티브 요소 Tab 포커스 가능
- 포커스 시 외곽선(outline) 명시적으로 표시
- 이미지 alt 속성 필수 작성
- 스크린리더 대응 `aria-label`, `role` 속성 주요 컴포넌트 적용

---

### 7.9 날짜 / 시간 표시 형식

| 상황 | 형식 | 예시 |
|---|---|---|
| 날짜만 | YYYY.MM.DD | 2024.01.15 |
| 날짜+시간 | YYYY.MM.DD HH:mm | 2024.01.15 14:30 |
| 상대 시간 (24시간 이내) | N분 전 / N시간 전 | 3시간 전 |
| 상대 시간 (24시간 이상) | YYYY.MM.DD | 2024.01.14 |

---

### 7.10 파일 업로드 공통 정책

| 항목 | 정책 |
|---|---|
| 허용 확장자 | jpg, jpeg, png, gif, pdf, xlsx, docx |
| 최대 파일 크기 | 이미지 10MB / 문서 50MB |
| 다중 업로드 | 최대 5개 동시 업로드 |
| 드래그앤드롭 | 지원 |
| 진행률 표시 | 업로드 Progress Bar 표시 |
| 에러 | 크기 초과 / 형식 불일치 시 인라인 에러 메시지 |

---

## 8. 빌드 및 배포

### 8.1 환경 변수

**Frontend (.env)**

```env
VITE_API_BASE_URL=https://api.example.com
VITE_GA_TRACKING_ID=G-XXXXXXXXXX
VITE_CHANNEL_TALK_KEY=xxxxxxxx
```

**Backend (.env)**

```env
# Server
PORT=3000
NODE_ENV=production

# Database
DATABASE_URL=postgresql://user:password@host:5432/dbname

# Auth
JWT_SECRET=your-secret-key
JWT_ACCESS_EXPIRES_IN=15m
JWT_REFRESH_EXPIRES_IN=7d

# Redis
REDIS_HOST=localhost
REDIS_PORT=6379

# AWS
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_S3_BUCKET=your-bucket
AWS_REGION=ap-northeast-2
```

### 8.2 빌드 명령어

**Frontend**

```bash
# 의존성 설치
npm install

# 개발 서버 실행
npm run dev

# 프로덕션 빌드
npm run build

# 빌드 결과 미리보기
npm run preview

# 린트 검사
npm run lint

# 타입 검사
npm run type-check
```

**Backend**

```bash
# 의존성 설치
npm install

# 개발 서버 실행 (hot-reload)
npm run start:dev

# 프로덕션 빌드
npm run build

# 프로덕션 실행
npm run start:prod

# 테스트 실행
npm run test

# DB 마이그레이션
npm run migration:run
```

### 8.3 배포 파이프라인

```
코드 Push (GitHub)
       │
       ▼
GitHub Actions CI
├── Lint 검사
├── Type Check
├── Unit Test
└── Build 성공 여부 확인
       │
       ▼
   [dev 브랜치]         [main 브랜치]
       │                     │
       ▼                     ▼
  dev 환경 배포         staging 배포
  (자동)               (자동)
                            │
                            ▼
                       수동 승인
                            │
                            ▼
                      production 배포
```

### 8.4 브랜치 전략

| 브랜치 | 용도 | 배포 환경 |
|---|---|---|
| main | 운영 배포용 | production |
| develop | 통합 개발 브랜치 | dev |
| feature/xxx | 기능 개발 | local |
| hotfix/xxx | 긴급 수정 | - |
| release/x.x.x | 릴리즈 준비 | staging |

### 8.5 롤백 절차

1. 이상 감지 → 담당자 즉시 알림
2. 이전 버전 이미지 태그 확인
3. ECS Task Definition 이전 버전으로 업데이트
4. 배포 완료 확인
5. 원인 분석 및 포스트모템 작성

---

## 9. 부록

### 9.1 참고 문서

- [Figma 디자인 시안](링크)
- [ERD 다이어그램](링크)
- [API 문서 (Swagger)](링크)
- [요구사항 정의서](링크)

### 9.2 변경 이력

| 버전 | 날짜 | 작성자 | 변경 내용 |
|---|---|---|---|
| v1.0.0 | YYYY-MM-DD | - | 최초 작성 |
| v1.1.0 | YYYY-MM-DD | - | - |

### 9.3 미결 사항 (Open Issues)

| # | 내용 | 담당 | 마감일 | 상태 |
|---|---|---|---|---|
| 1 | 소셜 로그인 제공사 범위 확정 필요 | - | YYYY-MM-DD | 🔴 미결 |
| 2 | 파일 업로드 최대 용량 정책 확인 | - | YYYY-MM-DD | 🟡 검토중 |
